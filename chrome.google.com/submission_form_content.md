# Chrome web store submission draft

## Store listing tab

Title from package:

Browser Controller

Summary from package:

Native messaging bridge for browser-controller CLI

Description:

Provides a way to inspect and control browser windows and tabs from a Rust CLI
program. It also allows watching an event stream of window and tab events. The
Event stream and CLI output use JSON for simple scripting with e.g. jq

This is meant to allow a power user to integrate browser window and tab control
with other parts of their environment, e.g. shell scripts, window manager or
compositor shortcuts, system services or timed jobs.

For ease of scripting the output can optionally use JSON and the event stream of
browser events uses newline delimited JSON objects.

For windows it allows listing, opening (including incognito windows) and closing
windows. Window title prefix management is a Firefox-only feature and is not
available on Chrome.

For tabs it allows listing, opening, activating, navigating to a new URL,
closing, reloading (with optional cache bypass), pinning, unpinning, discarding
tabs to free memory, muting and unmuting tab audio, moving the tab to a
different position in the window's tab bar, going forward and backward in
history, and sorting tabs by domain.

Some tab features are Firefox-only and return an error on Chrome: toggling
Reader Mode, warming up discarded tabs, and reopening tabs in containers.
Container management (listing containers) is also Firefox-only.

For downloads it allows listing (with optional state and query
filters), starting new downloads, cancelling, pausing, resuming,
retrying interrupted downloads,
and erasing download history entries.

When opening a new tab it can also optionally provide credentials for HTTP basic
authentication via the browser's onAuthRequired API, avoiding the need to embed
credentials in the URL where they would be visible in the address bar, history,
and logs.

The event stream includes events for window and tab operations (open, close,
activate, navigate, title change, status change) as well as download events
(created, changed, erased).

Note: My primary testing and development platform for this is Linux but I do
provide binaries for other desktop platforms for the Rust side of this. Bug
reports from users on those platforms are welcome if I overlooked some minor
platform-specific issues. This is not really meant for use on mobile platforms
since CLI use is uncommon there.

Category:

Tools

Language:

English

Homepage URL

<https://github.com/taladar/browser-controller>

## Privacy tab

Single purpose:

Retrieve information about and control tabs and windows from the command-line.

alarms justification:

Used as a safety net to ensure the native messaging connection is re-established
after an unexpected service worker termination. Chrome MV3 service
workers can be terminated after periods of inactivity, which drops
the native messaging connection to the mediator binary. A single-shot
alarm is scheduled when the connection drops, and the onAlarm listener
reconnects if the port is still null when the alarm fires. The alarm
is cleared immediately on successful reconnection. No periodic alarms
are used -- only on-demand alarms triggered by disconnection events.
This complements the offscreen document keepalive as a
belt-and-suspenders approach to connection reliability.

offscreen justification:

Creates a minimal offscreen document that sends a periodic keepalive message to
the service worker every 20 seconds. Each message resets Chrome's 30-second idle
timer for the service worker, preventing it from being terminated
while the native messaging connection is active. This is necessary
because Chrome MV3 can terminate service workers despite an active
connectNative() port (known Chrome bugs). The offscreen document
contains no user-visible UI, does not access any user data, and its
sole purpose is sending the keepalive message via
chrome.runtime.sendMessage().

nativeMessaging justification:

The extension connects to a locally-installed native messaging host binary
(browser-controller-mediator) that acts as an IPC bridge. The mediator receives
commands from the browser-controller CLI tool over a Unix domain socket (or
Windows named pipe) and forwards them to the extension via native messaging.
Responses and browser events flow back through the same channel. This is the
core communication mechanism -- without it, the CLI cannot interact with the
browser. The mediator binary must be separately installed by the user and
registered via a native messaging manifest. No data leaves the local machine.

scripting justification:

Used for two specific purposes in tab history navigation. First, to read
window.history.length and window.navigation.entries() (Navigation API) to report
accurate back/forward step counts in tab details -- this tells the CLI user how
many history steps are available in each direction. Second, to execute
window.history.go(delta) for the GoBack and GoForward commands, which navigate a
tab's session history by a given number of steps. Scripts are only injected
on-demand when these specific commands are invoked, not persistently. The
injected code is minimal (a few lines) and bundled in the extension -- no remote
code is fetched or executed.

tabs justification:

Required to implement the core tab management functionality. Read operations:
query all tabs in a window to list them with metadata (URL, title, loading
status, pinned/muted state, audible, discarded, active). Write operations:
create new tabs (with optional URL, position, and background mode), close tabs,
activate (switch to) a tab, navigate a tab to a new URL, reload with optional
cache bypass, pin/unpin, mute/unmute audio, move tabs to different positions
within a window, and discard tabs to free memory. All operations are
initiated by
explicit CLI commands from the local user -- nothing happens automatically or in
the background.

webRequest justification:

Listens to onAuthRequired events to provide two features. First, when the CLI
opens a tab with username/password credentials, the onAuthRequired listener
intercepts the server's 401 challenge and supplies the stored credentials
programmatically -- this avoids embedding credentials in the URL where they'd be
visible in the address bar, history, and logs. Credentials are held in memory
only briefly (30-second timeout) and are never persisted. Second,
onAuthRequired,
onCompleted, and onErrorOccurred listeners track which tabs are currently
awaiting HTTP authentication, reported as a boolean flag in tab details so the
CLI user knows when auth is pending.

Host permission justification:

Required by two other permissions. The webRequest API needs host permissions to
observe HTTP authentication challenges (onAuthRequired) on any URL the user
navigates to -- restricting to specific hosts would break credential injection
for arbitrary sites. The scripting API needs host permissions to inject the
history navigation content script into any tab, since the user may want to
navigate history in tabs showing any domain. No content is modified,
exfiltrated,
or sent externally -- all data stays local.

downloads justification:

Provides download management from the command line. Read operations: list
downloads with optional filtering by state (in_progress, complete, interrupted)
and search query. Write operations: start new downloads (with optional filename,
save-as dialog, and conflict resolution), cancel in-progress
downloads, pause and
resume downloads, retry interrupted downloads, and erase download history
entries. All operations are initiated by explicit CLI commands from the local
user.

Are you using remote code?

No, I am not using remote code

What user data do you plan to collect from users now or in the future?

None

I certify that the following disclosures are true:

I do not sell or transfer user data to third parties, outside of the approved
use cases

I do not use or transfer user data for purposes that are unrelated to my item's
single purpose

I do not use or transfer user data to determine creditworthiness or for lending
purposes
