/** Normalized event envelope from Rust backend. */
export interface AppEvent<T = unknown> {
  type: string;
  timestamp: string;
  payload: T;
}
