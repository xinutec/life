/** How often replication pulls. */
export const PULL_INTERVAL_MS = 60_000;

/** Five missed pulls: past this the device may be showing old data. */
export const STALE_AFTER_MS = 5 * PULL_INTERVAL_MS;
