# When the connection drops

The session collector saves changed turns before uploading. SDK adapters likewise
acknowledge local storage separately from cloud delivery. An upload failure keeps
unacknowledged records queued; a retry uses their existing identities.

This development increment adds a saved delivery cooldown. Restarting the collector
or running another flush does not skip that wait. Ordinary failures use exponential
backoff with random variation: initially 5–10 seconds, increasing up to 150–300
seconds. A valid server `Retry-After` delay or HTTP date can require a longer wait.
See [HTTP Retry-After](https://www.rfc-editor.org/rfc/rfc9110.html#name-retry-after).
This timing applies to uploads to RROFLOW, not calls to your AI provider.

## What you can check

Run `rroflow sources status` for session collection, or
`rroflow capture status --spool "YOUR SPOOL PATH"` for SDK recording. The `retry`
section shows `next_attempt_unix` (UTC seconds), attempts since the last success (including a reserved or in-progress attempt) and the last HTTP
status when available. Zero means no saved cooldown. A future time means delivery
is waiting; it does not mean captured work was deleted. `sources flush` and SDK
`capture.flush()` respect the same saved wait and return an error while deferred.

Session collection keeps scanning while delivery waits. Its managed worker retries
when eligible. The SDK recorder also accepts new events during the cooldown, but
an SDK flush remains an explicit attempt: the application must schedule another
flush after a deferred or failed one. Closing the application does not create a
background SDK uploader. The automatic session collector is a separate mechanism.

Source health requests share the session collector's cooldown. The dashboard may
therefore show an older report while the server asks the collector to wait. The
cloud cannot see an offline machine's newest local queue. This increment does not
add live retry-time display to the dashboard.

## Recovery rules

Only a verified receipt acknowledges records. Invalid receipts, account mismatch,
connection failures and HTTP errors retain the queue. The retry file contains
only counters, a deadline and an HTTP status, never tokens or captured text. It is
written atomically before an attempt so an interrupted process retains a cooldown.
A successful attempt clears it; malformed retry metadata fails closed rather than
silently resetting pacing. Accurate system time is required for saved deadlines.

This is not unlimited storage or proof of complete capture. Disk limits, retention,
large-backlog performance and every provider surface still need separate work.
Native fixture tests and release checks are the verification source for this
increment; they do not establish Texas deployment or personal account activation.
