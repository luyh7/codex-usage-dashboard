import type { Page, TaskGroup } from './types';

export interface ProjectTasksState {
  page: Page<TaskGroup> | null;
  loaded: string;
  loadedQuery: string;
  showAll: boolean;
}

/** Retain recently visible projects when virtual rows leave the viewport. */
export class ProjectCache {
  private entries = new Map<string, ProjectTasksState>();
  get(key: string) { return this.entries.get(key); }
  set(key: string, value: ProjectTasksState) {
    this.entries.delete(key);
    this.entries.set(key, value);
    if (this.entries.size > 80) this.entries.delete(this.entries.keys().next().value!);
  }
}
