# Rust TUI Web Page Monitor

This Rust application is a **terminal-based web page monitor** that checks a URL periodically and reports changes to a specific HTML element or the whole page. It provides a **cargo-style setup**, live monitoring, change logging, colored diff output, and URL status reporting.

Built with assistance of OpenAI GPT-5.3 free plan.

---

## Features

- **Sequential setup wizard:** Ask for URL → CSS selector → interval (seconds)
- **Live monitoring:** Periodically fetches the page
- **Change detection:** Highlights the exact HTML differences
- **Baseline updates:** Future comparisons use the latest page
- **URL unreachable detection**
- **Dynamic content filtering:** Ignores timestamps and predictable dynamic content
- **TUI Interface:** Built with `ratatui` + `crossterm`
- **Colored diff:** Removed lines in red, added lines in green
- **Scrollable change log**
- **Quit and navigation controls:** `q` to quit, `Enter` to proceed

---

## Crates Used

| Crate       | Purpose                                               |
|------------|-------------------------------------------------------|
| `tokio`     | Async runtime                                       |
| `reqwest`   | HTTP requests                                       |
| `scraper`   | HTML parsing                                       |
| `sha2`      | Optional hash comparison                            |
| `ratatui`   | Terminal UI                                        |
| `crossterm` | Terminal input/output control                        |
| `similar`   | HTML diff generation                                |
| `regex`     | Dynamic content filtering                           |
| `anyhow`    | Error handling                                     |
| `chrono`    | Timestamping                                       |

---

## File Structure

`main.rs` contains:

1. **Enums and Structs**
   - `Stage`: Represents the current input stage (URL, Selector, Interval, Running)
   - `MonitorEvent`: Sent from the monitor task to the TUI (`Checked`, `Changed(diff)`, `Unreachable`)
   - `App`: Holds TUI state, last check, status, and change log

2. **Functions**
   - `fetch_content(url, selector)`: Fetches and optionally filters a specific element
   - `clean_html(content)`: Removes dynamic HTML parts (timestamps, dates) before comparison
   - `generate_diff(old, new)`: Produces a colored diff as `Vec<Line>` for TUI display
   - `monitor(url, selector, interval, tx)`: Async loop to fetch content, compare, generate diff, and send events
   - `draw(frame, app)`: Draws the TUI with status panel, change log, and footer

3. **Main Loop**
   - Handles TUI input events: Enter, Backspace, q to quit
   - Updates status and change log from monitor events
   - Starts the monitor task asynchronously after interval input

---

## Example Usage

1. Run the app:

```bash
cargo run

Enter URL: https://example.com
Enter Selector (default html): #price
Enter Interval (seconds): 30

┌ Monitoring ──────────────────────────────┐
URL: https://example.com
Selector: #price
Interval: 30 sec
Last Check: 14:32:01
Status: CHANGE DETECTED