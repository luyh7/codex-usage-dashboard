export interface Usage {
  input_tokens: number;
  cached_input_tokens: number;
  cache_write_tokens: number;
  output_tokens: number;
  reasoning_output_tokens: number;
  total_tokens: number;
}
export interface SpeedTotals {
  first_token_request_count?: number;
  output_tokens: number;
  duration_ms: number;
  request_count: number;
}
export interface Session {
  estimated_speed?: SpeedTotals | null;
  uid: string;
  session_id: string;
  root_session_id: string;
  parent_thread_id: string;
  forked_from_id: string;
  is_subagent: boolean;
  agent_path: string;
  agent_nickname: string;
  agent_role: string;
  title: string;
  source: string;
  environment: string;
  environment_id: string;
  is_remote: boolean;
  remote_device_short_code: string;
  path: string;
  cwd: string;
  project: string;
  project_root: string;
  workspace_root: string;
  project_branch: string;
  is_git_worktree: boolean;
  model: string;
  models: string[];
  service_tier: string;
  service_tiers: string[];
  effort: string;
  created_at: string;
  start_at: string;
  end_at: string;
  updated_at: string | null;
  last_usage_at?: string | null;
  total_token_usage: Usage;
  last_token_usage: Usage;
  inherited_token_usage: Usage;
  branch_total_token_usage: Usage;
  cached_input_percent: number | null;
  estimated_cost_usd: number | null;
  estimated_cost_breakdown_usd: Partial<Usage> | null;
  price_model_known: boolean;
  token_event_count: number;
  turn_count: number;
  completed_turn_count: number;
  duration_ms_total: number;
  time_to_first_token_ms_avg: number | null;
}
export interface ModelBreakdown {
  model: string;
  usage: Usage;
  estimated_cost_usd: number | null;
  cost_complete: boolean;
}
export interface Summary {
  estimated_speed?: SpeedTotals | null;
  session_count: number;
  main_count?: number;
  agent_count?: number;
  active_count: number;
  archived_count: number;
  usage: Usage;
  estimated_cost_usd: number;
  estimated_cost_known_count: number;
  by_model: { model: string; sessions: number; usage: Usage }[];
  model_breakdown?: ModelBreakdown[];
  by_environment: { id: string; label: string; sessions: number; usage: Usage }[];
}
export interface Project {
  estimated_speed?: SpeedTotals | null;
  updated_at?: string | null;
  key: string;
  label: string;
  session_count: number;
  usage: Usage;
  estimated_cost_usd: number;
  estimated_cost_known_count: number;
  environments: string[];
}
export interface TaskGroup {
  identity: string;
  sessions: Session[];
  summary: Summary;
}
export interface Page<T> {
  query_token: string;
  revision: string;
  items: T[];
  summary: Summary;
  total: number;
  offset: number;
  limit: number;
  next_cursor: string | null;
  previous_cursor: string | null;
}
export interface Period {
  key: string;
  start_at: string | null;
  end_at: string;
  start_date: string | null;
  end_date: string | null;
}
export interface Source {
  id: string;
  label: string;
  codex_home: string;
  is_remote: boolean;
}
export interface Remote {
  device_short_code: string;
  label: string;
  platform: string;
  hostname: string;
  session_count: number;
  usage: Usage;
  imported_at: string;
  exported_at: string;
  generated_at: string;
}
export interface LocalDevice {
  device_short_code: string;
  label: string;
  session_count: number;
  sources?: { id: string; label: string; session_count: number }[];
}
export interface Listing<T> {
  current_device?: LocalDevice;
  metric_trend?: MetricTrend;
  facets?: Summary;
  page: Page<T>;
  period: Period;
  generated_at: string;
  codex_sources: Source[];
  remotes: Remote[];
  current_device_short_code: string;
}
export interface MetricTrend {
  complete: boolean;
  granularity?: 'day' | 'hour';
  points: {
    date: string;
    end?: string;
    usage: Usage;
    session_count: number;
    estimated_cost_usd: number | null;
  }[];
}
export interface Query {
  offset?: number;
  period: string;
  start: string;
  end: string;
  timezone: string;
  search: string;
  model: string;
  source: string;
  environment: string;
  agents: string;
  sort: string;
  ascending: boolean;
}
export interface TokenEvent {
  generation_duration_ms?: number | null;
  request_duration_ms?: number | null;
  timestamp: string | null;
  model: string;
  service_tier: string;
  total_token_usage: Usage;
  last_token_usage: Usage | null;
  model_context_window: number | null;
  rate_limits: unknown;
}
export interface Events {
  revision: string;
  id: string;
  events: TokenEvent[];
  offset: number;
  total: number;
  limit: number;
  next_cursor: string | null;
  previous_cursor: string | null;
}
export interface Chart {
  revision: string;
  id: string;
  timeline: { timestamp: string | null; total_token_usage: { total_tokens: number } }[];
}
export interface Detail extends Session {
  has_subagents?: boolean;
  revision: string;
  first_user_prompt: string;
  last_agent_preview: string;
  tool_counts: Record<string, number>;
  parse_errors: number;
  originator: string;
  cli_version: string;
  file_size: number;
  fork_usage_resolved: boolean;
  inherited_token_event_count: number;
  applied_price_segments: {
    model: string;
    effective_at?: string | null;
    effective_until?: string | null;
    service_tier: string;
    estimated_cost_usd: number | null;
    price_known: boolean;
    usage: Usage;
    [key: string]: unknown;
  }[];
  tasks: {
    timestamp: string | null;
    duration_ms: number | null;
    time_to_first_token_ms: number | null;
  }[];
}
export interface DetailBundle {
  detail: Detail;
  events: Events;
  chart: Chart;
}
export interface Daily {
  generated_at: string;
  daily_usage: { date: string; usage: Usage }[];
  daily_usage_complete: boolean;
}
export interface IndexStatus {
  phase: string;
  revision: number;
  completed?: number;
  total?: number;
  error?: string;
}
export interface SnapshotImportState {
  phase: 'idle' | 'working' | 'confirmation' | 'success' | 'error';
  startedAt?: number;
  error?: string;
  incoming?: unknown;
  label?: string;
  confirmation?: ImportResult;
}
export interface ImportResult {
  ok: boolean;
  needs_label?: boolean;
  needs_confirmation?: boolean;
  device_short_code?: string;
  suggested_label?: string;
  reason?: string;
  remote?: Remote;
}
/** Host-neutral seam. UI components receive this interface, never fetch/Tauri. */
export interface DashboardData {
  projects(
    query: Query,
    cursor?: string,
    revision?: string,
    conditional?: string,
    limit?: number,
  ): Promise<Listing<Project> | null>;
  sessions(
    query: Query,
    cursor?: string,
    revision?: string,
    conditional?: string,
  ): Promise<Listing<Session> | null>;
  projectTasks(
    query: Query,
    project: string,
    revision: string,
    cursor?: string,
    limit?: number,
  ): Promise<Page<TaskGroup>>;
  detail(id: string, revision: string, query?: Query): Promise<DetailBundle>;
  events(id: string, revision: string, cursor: string, offset?: number): Promise<Events>;
  daily(timezone: string): Promise<Daily>;
  importSnapshot(snapshot: unknown, label?: string, allowCurrent?: boolean): Promise<ImportResult>;
  renameRemote(code: string, label: string): Promise<void>;
  deleteRemote(code: string): Promise<void>;
  subscribe(onStatus: (status: IndexStatus) => void, onReconnect: () => void): () => void;
  forceRefresh(): Promise<void>;
  exportUrl(mode?: 'full' | 'redacted'): string;
  cancelDetails(): void;
  cancelGroups(): void;
  close(): void;
}
