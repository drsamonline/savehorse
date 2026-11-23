/** Shapes the Rust side sends over the bridge.
 *
 * These mirror `savehorse-registry` exactly. If the manifest changes shape,
 * this file changes with it — which is why there is one manifest and not a
 * second copy in TypeScript.
 */

export type ParamKind =
  | "Path"
  | "Paths"
  | "Text"
  | "Pattern"
  | "Number"
  | "Size"
  | "Bool"
  | "Choice"
  | "Secret";

export type DangerLevel = "safe" | "destructive" | "irreversible";
export type OpStatus = "implemented" | "planned";

export interface ParamView {
  name: string;
  kind: ParamKind;
  positional: boolean;
  required: boolean;
  help: string;
  options: string[];
}

export interface OpView {
  id: string;
  name: string;
  summary: string;
  danger: DangerLevel;
  status: OpStatus;
  params: ParamView[];
}

export interface ModuleView {
  name: string;
  title: string;
  job: string;
  sources: string;
  ops: OpView[];
}

export interface RunRequest {
  module: string;
  op: string;
  positional: string[];
  flags: Record<string, string>;
}

export interface RunResult {
  text: string;
  json: string;
  dry_run: boolean;
}

/** One entry in the job dock. */
export interface Job {
  id: number;
  at: string;
  label: string;
  state: "ok" | "error" | "preview";
  detail: string;
  ms: number;
  request: RunRequest;
}

export const DANGER_LABEL: Record<DangerLevel, string> = {
  safe: "reads only",
  destructive: "writes files",
  irreversible: "cannot be undone",
};
