# Collector account storage v1

Status: implemented on the development branch; native device authorization is still open.
This contract owns local credential update behavior. Account identity and destination
selection remain in [identity and devspaces](../plan/identity-and-devspaces.md).

## Concurrent commands

A background collector, SDK flush and foreground command can share an account. A
stable `account.json.lock` file provides an operating-system exclusive lock from
reading a credential through refresh and durable replacement. Logout uses the same
lock through revocation and removal. The lock file is never removed or renamed;
otherwise processes could lock different inodes. Process exit releases ownership.
A competing operation fails promptly with a retry message. It does not spend the
refresh token, rewrite the account or remove the other process's sign-in.

The account/destination check occurs on the same locked snapshot before refresh.
`whoami` also checks the configured destination before making credential-bearing
requests. Explicitly bound background collectors continue using their saved
account reference; ambient deployment settings cannot reroute their queue.

Device login releases the lock while the person approves in a browser. Its final
write compares both the original account and a durable mutation marker in the lock
file. A competing login, refresh or logout invalidates that snapshot. Logout even
when no account exists changes the marker, so it cancels a pending first login.
A refused save attempts to revoke the newly acquired tokens. The person can start
a new login after the competing operation finishes.

## Files and interrupted work

Account reads and writes are limited to 64 KiB. Errors never include file contents.
The account and lock paths must be regular files; symlinks are refused. The config
directory must be private to the operating-system user. This is not protection
against a compromised process running as that same user.

Every write creates its own unpredictable temporary file using exclusive creation,
writes and synchronizes it, then atomically replaces the account file. It never
truncates a shared `account.json.tmp` file left by another or interrupted writer.
A failure before replacement leaves the committed account intact and removes only its
own scratch file when possible. If a Unix directory sync fails after replacement, the
command reports failure; a later read uses the actual committed account file. A crash can leave a private, ignored temporary file. The account
file is never reconstructed from scratch files.

New Unix directories use mode 0700; lock and temporary files use mode 0600 at
creation, before credential bytes are written. Successful Unix replacement/removal
also synchronizes the parent directory. Windows inherits the configured directory's
ACL; this change does not configure an ACL or promise protection in a shared folder.
The tests cover process termination and ordinary atomic replacement, not physical
power-loss behavior on every filesystem. Network filesystems are not qualified.

Explicit logout can remove a corrupt regular account file without trying to use its
contents as credentials. Remote revocation cannot be performed in that case.

## Delivery and remaining device work

An account-lock refusal follows the collector's existing durable delivery retry
path. Capture can keep recording while delivery waits. The process test queues two
SDK events while another process refreshes, then verifies both are acknowledged
using the saved fresh credential after the lock and delivery cooldown clear.
The issuer and ingest in this test are disposable protocol fixtures, not real AI
provider traffic or deployed RROFLOW accounts.

Serialization alone does not recover a refresh response lost after the issuer has
rotated a token. Native device authorization must still implement revocable,
account-bound grants, bounded approval/polling, and durable retry identity with
server-side replay handling before it is considered complete. Existing Nomen
refresh behavior remains bounded by that issuer's policy. The native CLI must not
ask for a member's password or silently turn a browser session into a device grant.
The protocol inputs are [RFC 8628](https://www.rfc-editor.org/rfc/rfc8628.html)
and [RFC 9700 section 4.14](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.14).

Rust tests cover bounded private replacement, stale scratch preservation, lock
ownership during validation, cancellation of an in-flight first login, and Unix
symlink refusal. Real CLI subprocess tests cover concurrent refresh/logout,
termination, corrupt-account logout, durable SDK replay, and deployment mismatch
before refresh. CI runs these processes on Linux and Windows, x64 and ARM64.
