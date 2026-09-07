export interface Discard {
  tile: string;
  tsumogiri: boolean;
  riichi: boolean;
  called: boolean;
}
export interface Meld {
  kind: string;
  tiles: string[];
  called: string | null;
  from: number | null;
}
export interface Player {
  score: number;
  riichi: boolean;
  concealed: string[];
  discards: Discard[];
  melds: Meld[];
}
export interface GameEvent {
  kind: string;
  actor: number | null;
  target: number | null;
  tile: string | null;
}
export interface Frame {
  event_index: number;
  round: string;
  honba: number;
  dealer: number;
  riichi_sticks: number;
  remaining_draws: number;
  dora_indicators: string[];
  active_player: number | null;
  settled: boolean;
  drawn: [number, string] | null;
  players: Player[];
  event: GameEvent;
}
export interface Replay {
  id: number;
  mortal_supported: boolean;
  names: string[];
  frames: Frame[];
  rounds: { frame_index: number; label: string }[];
}
export interface Action {
  type: string;
  actor?: number;
  target?: number;
  pai?: string;
}
export interface Efficiency {
  discard: string;
  shanten: number;
  total_unseen: number;
  draw_kind: string;
  draws: { tile: string; unseen: number }[];
}
export interface Evidence {
  event_index: number;
  player: number;
  discards: Efficiency[];
  mortal: {
    model: { version: number; tag: string; sha256: string };
    decision: {
      recommended: Action;
      candidates: { action: { kind: string; tile?: string }; q_value: number }[];
      kan_candidates: { tile: string; q_value: number }[];
      shanten: number;
      at_furiten: boolean;
    } | null;
  };
}
export interface Decision {
  event_index: number;
  turn: number;
  actual: { kind: string; action?: Action };
  evidence: Evidence;
}
export interface Bridge {
  getSettings(): Promise<Settings>;
  saveSettings(input: SettingsInput): Promise<Settings>;
  testConnection(input: SettingsInput): Promise<void>;
  runtimeStatus(check: boolean): Promise<RuntimeStatus>;
  importLog(json: string): Promise<Replay>;
  importLink(link: string): Promise<Replay>;
  majsoulStatus(): Promise<boolean>;
  loginMajsoul(input: { username: string; password: string; accept_risk: boolean }): Promise<void>;
  logoutMajsoul(): Promise<void>;
  analyze(id: number, player: number): Promise<Decision[]>;
  ask(
    id: number,
    player: number,
    event: number,
    conversation: string,
    text: string,
    context_label: string,
  ): Promise<SessionView>;
  listSessions(): Promise<{ sessions: SessionSummary[]; warnings: string[] }>;
  getSession(id: string): Promise<SessionView>;
  continueSession(id: string, text: string): Promise<SessionView>;
  importSession(json: string): Promise<SessionView>;
  exportSession(id: string): Promise<string>;
}

export interface Settings {
  endpoint: string;
  model: string;
  has_api_key: boolean;
  saved: boolean;
}

export interface SettingsInput {
  endpoint: string;
  model: string;
  api_key: string;
  clear_key: boolean;
}

export interface RuntimeStatus {
  bundled: boolean;
  available: boolean;
  checked: boolean;
  model: string;
}

export interface SessionTurn {
  question: string;
  answer: string | null;
  error: string | null;
  trace: { kind: string; [key: string]: unknown }[];
}
export interface SessionView {
  id: string;
  title: string;
  context_label: string;
  created_at: number;
  updated_at: number;
  busy: boolean;
  pending_question: string | null;
  archive: {
    version: number;
    instructions: string;
    tools: unknown[];
    endpoint: string;
    model: string;
    evidence: Evidence;
    history: unknown[];
    turns: SessionTurn[];
  };
}
export interface SessionSummary {
  id: string;
  title: string;
  context_label: string;
  updated_at: number;
  busy: boolean;
  interrupted: boolean;
  failed: boolean;
}
