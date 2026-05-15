export interface GitSyncAdapter {
  pull(): Promise<void>;
  push(): Promise<void>;
}

export function createGitSyncAdapter(): GitSyncAdapter {
  throw new Error("Git sync is intentionally not implemented yet.");
}
