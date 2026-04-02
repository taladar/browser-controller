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

Currently, as of 0.1.5 it allows listing, opening and closing windows.

For tabs it allows listing, opening, activating, navigating to a new URL,
closing, pinning, unpinning, warming discarded tabs up, muting and unmuting tab
audio, moving the tab to a different position in the window's tab bar and going
forward and backward in history.

When opening a new tab it can also optionally remove credentials embedded in the
URL of basic auth pages after they have been cached by the browser so they can't
be seen over the user's shoulder or accidentally copied.

The event stream includes events for basic window and tab operations like
opening, closing and activating another tab as well as changes in titles and
notifications when a page has finished loading.

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

nativeMessaging justification:

Connect to the locally-installed browser-controller-mediator binary to receive
and respond to CLI commands.

scripting justification:

Inject a content script to read window.history and window.navigation
for accurate back/forward step counts, and to execute history.go() for
GoBack/GoForward commands.

sessions justification:

Persist per-window title prefixes across browser restarts using
sessions.setWindowValue / getWindowValue (Firefox only).

tabs justification:

Read tab metadata (URL, title, status, pin/mute state) and perform operations:
open, close, activate, navigate, pin, mute, and move tabs.

webRequest justification:

Observe onAuthRequired events to track which tabs are awaiting HTTP
authentication, reported in tab details.

Host permission justification:

Required by webRequest to observe authentication events on any URL, and by
scripting to inject the history content script into any tab.

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
