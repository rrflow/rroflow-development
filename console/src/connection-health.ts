import type { Sources } from "./monitoring-api";

/** A report is a last observation, never a live collector status. */
export function connectionHealth(data: Sources, elapsedMs: number) {
  return {
    stale: data.rows.filter((row) => {
      const age = Date.parse(data.asOf) - Date.parse(row.receivedAt) + elapsedMs;
      return age < 0 || age > data.recentFor * 1000;
    }).length,
    problems: data.rows.filter((row) =>
      row.delivery === false || row.captureErrors > 0 ||
      (row.sdk !== null && (row.sdk.errors > 0 || row.sdk.retrying > 0)),
    ).length,
    pendingTurns: data.rows.reduce((total, row) => total + row.pending, 0),
  };
}
