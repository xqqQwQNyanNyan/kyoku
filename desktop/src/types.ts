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
  game_key: string;
  name?: string;
  mortal_supported: boolean;
  names: string[];
  frames: Frame[];
  rounds: { frame_index: number; label: string; result?: RoundResult | null }[];
}
export interface RoundResult {
  wins: [number, number][];
  deltas: number[];
  scores: number[];
  details:
    | {
        kind: 'hora';
        wins: { actor: number; target: number; score: string | null; yaku: string[] }[];
      }
    | { kind: 'ryukyoku'; reason: string }
    | null;
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
export interface StorageLocation {
  directory: string;
  available: boolean;
}
export interface MigrationProgress {
  copied_files: number;
  total_files: number;
  copied_bytes: number;
  total_bytes: number;
}
export interface Bridge {
  getStorage(): Promise<StorageLocation>;
  chooseDataDirectory(): Promise<string | null>;
  migrateData(
    directory: string,
    requestId: string,
    onProgress: (progress: MigrationProgress) => void,
  ): Promise<StorageLocation>;
  cancelDataMigration(requestId: string): Promise<void>;
  getSettings(): Promise<Settings>;
  saveSettings(input: SettingsInput): Promise<Settings>;
  testConnection(
    input: SettingsInput,
    onProgress: (progress: QuestionProgress) => void,
  ): Promise<void>;
  runtimeStatus(check: boolean): Promise<RuntimeStatus>;
  importLog(json: string, name: string): Promise<Replay>;
  importLink(link: string): Promise<Replay>;
  listReplays(): Promise<ReplayList>;
  previewReplayDeletion(key: string): Promise<{ name: string; session_ids: string[] }>;
  deleteReplay(
    key: string,
    sessionIds: string[],
  ): Promise<{
    session_ids: string[];
    replay_deleted: boolean;
    error: { message: string } | null;
  }>;
  renameReplay(key: string, name: string): Promise<SavedReplay>;
  openReplay(key: string): Promise<Replay>;
  openDataDirectory(): Promise<void>;
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
    run: QuestionRun,
  ): Promise<SessionView>;
  cancelQuestion(id: string, requestId: string): Promise<void>;
  listSessions(): Promise<{ sessions: SessionSummary[]; warnings: string[] }>;
  getSession(id: string): Promise<SessionView>;
  deleteSession(id: string): Promise<void>;
  renameSession(id: string, title: string): Promise<SessionView>;
  openSessionGame(id: string): Promise<{ replay: Replay; name: string; position: SessionPosition }>;
  setSessionPosition(id: string, gameKey: string, position: SessionPosition): Promise<void>;
  continueSession(id: string, text: string, run: QuestionRun): Promise<SessionView>;
  retrySession(id: string, turn: number | undefined, run: QuestionRun): Promise<SessionView>;
  importSession(json: string): Promise<SessionView>;
  exportSession(id: string): Promise<string>;
}

export type QuestionProgress =
  | { phase: 'usage'; requests: RequestUsage[]; budget: number | null }
  | { phase: 'preparing' }
  | { phase: 'model'; request: number }
  | { phase: 'verifying'; request: number }
  | { phase: 'tool'; name: string };

export interface QuestionRun {
  requestId: string;
  onProgress(progress: QuestionProgress): void;
}

export interface TokenPrices {
  currency: string;
  input: number;
  output: number;
  cached_input: number | null;
}
export interface ModelOptions {
  max_output_tokens: number;
  context_tokens: number | null;
  thinking: 'default' | 'none' | 'minimal' | 'low' | 'medium' | 'high' | 'xhigh' | 'max';
  chat_token_limit: 'max_completion_tokens' | 'max_tokens';
  token_budget: number | null;
  prices: TokenPrices | null;
}
export interface RequestUsage {
  input_tokens: number | null;
  output_tokens: number | null;
  cached_input_tokens: number | null;
  reasoning_tokens: number | null;
  cost: number | null;
  prices: TokenPrices | null;
}
export interface Settings {
  endpoint: string;
  model: string;
  has_api_key: boolean;
  saved: boolean;
  options?: ModelOptions;
}

export interface SavedReplay {
  key: string;
  name: string;
  origin: 'example' | 'file' | 'link' | 'session';
  saved_at: number;
}

export interface ReplayList {
  replays: SavedReplay[];
  warnings: string[];
  directory: string;
}

export interface SettingsInput {
  endpoint: string;
  model: string;
  api_key: string;
  clear_key: boolean;
  options?: ModelOptions;
}

export interface RuntimeStatus {
  bundled: boolean;
  available: boolean;
  checked: boolean;
  model: string;
}

export interface SessionPosition {
  player: number;
  event_index: number;
}
export interface SessionEvidence extends SessionPosition {
  position?: { round: { wind: string; number: number }; honba: number };
}
export interface SessionTurn {
  usage?: RequestUsage[];
  options?: ModelOptions | null;
  question: string;
  answer: string | null;
  error: string | null;
  trace: { kind: string; [key: string]: unknown }[];
  evidence?: SessionEvidence | null;
}
export interface SessionView {
  id: string;
  game?: { key: string } | null;
  position?: SessionPosition | null;
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
    evidence: SessionEvidence;
    history: unknown[];
    turns: SessionTurn[];
  };
}
export interface SessionSummary {
  id: string;
  game_key?: string | null;
  title: string;
  context_label: string;
  updated_at: number;
  busy: boolean;
  interrupted: boolean;
  failed: boolean;
}
