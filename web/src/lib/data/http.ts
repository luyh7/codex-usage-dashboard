import type {
  DashboardData,
  Query,
  Listing,
  Project,
  Session,
  Page,
  TaskGroup,
  Detail,
  Events,
  Chart,
  Daily,
  ImportResult,
  IndexStatus,
} from './types';
export class DataError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
  get stale() {
    return this.status === 409;
  }
}
export class Superseded extends Error {
  constructor() {
    super('superseded request');
    this.name = 'AbortError';
  }
}
export const cancelled = (error: unknown) => error instanceof Error && error.name === 'AbortError';
/** Cancellation and response identity are enforced here, including same-session
 * away/back races. A successful response cannot revive a superseded view. */
export class HttpDashboard implements DashboardData {
  private active = new Map<string, AbortController>();
  private subscriptions = new Set<() => void>();
  constructor(
    private base = '',
    private transport: typeof fetch = globalThis.fetch.bind(globalThis),
  ) {}
  private cancel(prefix: string) {
    for (const [key, controller] of this.active) {
      if (key.startsWith(prefix)) {
        controller.abort();
        this.active.delete(key);
      }
    }
  }
  cancelDetails() {
    this.cancel('detail:');
    this.cancel('events:');
  }
  cancelGroups() {
    this.cancel('group:');
  }
  private async get<T>(
    channel: string,
    path: string,
    params: Record<string, string | boolean | number | undefined> = {},
    revision?: string,
  ): Promise<T | null> {
    this.active.get(channel)?.abort();
    const controller = new AbortController();
    this.active.set(channel, controller);
    const url = new URLSearchParams();
    for (const [key, value] of Object.entries(params)) {
      if (value !== undefined && value !== '') url.set(key, String(value));
    }
    try {
      const response = await this.transport(this.base + path + '?' + url, {
        cache: 'no-store',
        signal: controller.signal,
      });
      if (controller.signal.aborted || this.active.get(channel) !== controller)
        throw new Superseded();
      if (response.status === 204) return null;
      const body = await response.json();
      if (controller.signal.aborted || this.active.get(channel) !== controller)
        throw new Superseded();
      if (!response.ok)
        throw new DataError(response.status, body.error || `HTTP ${response.status}`);
      if (revision && (body.revision ?? body.page?.revision) !== revision)
        throw new DataError(409, 'snapshot stale');
      return body as T;
    } finally {
      if (this.active.get(channel) === controller) this.active.delete(channel);
    }
  }
  projects(query: Query, cursor = '', revision = '', conditional = '') {
    return this.get<Listing<Project>>(
      'listing',
      '/api/v2/projects',
      { ...query, cursor, revision, if_query_token: conditional, limit: 30 },
      revision || undefined,
    );
  }
  sessions(query: Query, cursor = '', revision = '', conditional = '') {
    return this.get<Listing<Session>>(
      'listing',
      '/api/v2/sessions',
      { ...query, cursor, revision, if_query_token: conditional, limit: 100 },
      revision || undefined,
    );
  }
  async projectTasks(query: Query, project: string, revision: string, cursor = '') {
    return (await this.get<Page<TaskGroup>>(
      'group:' + project,
      '/api/v2/project-tasks',
      { ...query, project, revision, cursor, limit: 30 },
      revision,
    ))!;
  }
  async detail(id: string, revision: string) {
    this.cancelDetails();
    const params = { id, snapshot_token: revision };
    const [detail, events, chart] = await Promise.all([
      this.get<Detail>('detail:meta', '/api/v2/session', params, revision),
      this.get<Events>('detail:events', '/api/v2/events', params, revision),
      this.get<Chart>('detail:chart', '/api/v2/chart', params, revision),
    ]);
    if (!detail || !events || !chart || detail.uid !== id || events.id !== id || chart.id !== id)
      throw new DataError(409, 'detail identity changed');
    return { detail, events, chart };
  }
  async events(id: string, revision: string, cursor: string, offset?: number) {
    const page = (await this.get<Events>(
      'events:page',
      '/api/v2/events',
      { id, snapshot_token: revision, cursor, offset },
      revision,
    ))!;
    if (page.id !== id) throw new DataError(409, 'event identity changed');
    return page;
  }
  async daily(timezone: string) {
    return (await this.get<Daily>('daily', '/api/daily-usage', { timezone }))!;
  }
  private async post<T>(path: string, body: unknown): Promise<T> {
    const response = await this.transport(this.base + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    const value = await response.json();
    if (
      !response.ok &&
      !(response.status === 409 && (value.needs_label || value.needs_confirmation))
    )
      throw new DataError(response.status, value.error || `HTTP ${response.status}`);
    return value;
  }
  importSnapshot(snapshot: unknown, label?: string, allowCurrent = false) {
    return this.post<ImportResult>('/api/remotes/import', {
      snapshot,
      label,
      allow_current_device: allowCurrent,
    });
  }
  async renameRemote(code: string, label: string) {
    await this.post('/api/remotes/rename', { device_short_code: code, label });
  }
  async deleteRemote(code: string) {
    await this.post('/api/remotes/delete', { device_short_code: code, confirm: true });
  }
  subscribe(onStatus: (status: IndexStatus) => void, onReconnect: () => void) {
    const stream = new EventSource(this.base + '/api/revisions');
    let connected = false;
    stream.onopen = () => {
      if (connected) onReconnect();
      connected = true;
    };
    stream.addEventListener('revision', (event) => {
      try {
        onStatus(JSON.parse((event as MessageEvent).data));
      } catch {
        /* interrupted SSE frame; reconnect and polling revalidate */
      }
    });
    const close = () => {
      stream.close();
      this.subscriptions.delete(close);
    };
    this.subscriptions.add(close);
    return close;
  }
  exportUrl() {
    return this.base + '/api/export.json';
  }
  close() {
    this.cancel('');
    for (const close of this.subscriptions) close();
  }
}
