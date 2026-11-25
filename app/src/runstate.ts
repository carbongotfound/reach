// "Is a run active?" One answer for the start screen's banner and spinner, the composers, the pickers and the sidebar.
// A run is active only while a turn is going ("running"). "waiting" means the turn ended and the task can take a
// reply: it shows as Done and never blocks a new task. Runs left over from an earlier app session are settled by
// the backend's list_runs (running -> failed, waiting -> done), so they never count.
export const active = (state: string) => state === "running";
export const activeRun = <R extends { state: string }>(runs: R[] | null | undefined) => runs?.find((r) => active(r.state));
