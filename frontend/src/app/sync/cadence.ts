/** How often replication asks, and how long silence is tolerated.
 *
 *  Both numbers live here because one is defined in terms of the other, and
 *  they were briefly duplicated: `replication.ts` polled every 60s while
 *  `sync-status.ts` carried its own `5 * 60_000` and a comment claiming that
 *  was five missed heartbeats. Nothing checked the claim, so moving the poll
 *  interval would have left the comment true-looking and false.
 */

/** The pull heartbeat. Without one, `live: true` performs a single pull and
 *  stops — see the note in `replication.ts` (#1567). */
export const PULL_INTERVAL_MS = 60_000;

/** Silence long enough to admit the device may be showing old data. Five missed
 *  heartbeats, stated as arithmetic so it stays five. Long enough that a slow
 *  network or a throttled tab does not trip it, short enough to notice before
 *  acting on a stale view. */
export const STALE_AFTER_MS = 5 * PULL_INTERVAL_MS;
