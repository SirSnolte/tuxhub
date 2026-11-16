import type { ClockLayout } from "./layout-model";

/**
 * Sends a new layout from the inspector. `key` names a gesture (a slider drag, typing in a
 * field) whose further steps join its undo step and go out after a pause; `refusal` is the
 * hint when the layout breaks a limit.
 */
export type Commit = (next: ClockLayout | null, key?: string | null, refusal?: string) => void;
