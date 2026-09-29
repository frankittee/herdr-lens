# Herdr Lens

Herdr Lens shows the conversation of the agent in the invoking Herdr pane in a read-only browser view.

Conversation Markdown renders inline math with `$...$` and display math with `$$` on separate lines. The browser bundles the required KaTeX fonts for offline viewing.
Hover a rendered formula to reveal **Copy**, which copies its LaTeX source.

When the Herdr server starts, Lens starts one shared viewer on its loopback interface. Use **Open agent conversation in browser** when Herdr runs on this machine. When attached with `herdr --remote`, use **Show agent conversation link for SSH forwarding** on the remote server. The actions select the invoking pane and reuse the running viewer. Herdr shows the remote URL in a notification; Lens does not try to open a browser on the remote host.

The viewer binds to `127.0.0.1` on the remote host. To reach it locally, use the port shown in the URL (for example, `12345`):

```sh
ssh -N -L 12345:127.0.0.1:12345 workbox
```

Then open the notified URL on this machine. If the local port is occupied, choose another local port and replace only the port in the URL. Herdr does not currently expose a remote-client flag or local clipboard API to plugin actions, so choose the link action explicitly for remote sessions. The startup hook runs when the Herdr server starts, not when a client attaches or the plugin is linked; restart the Herdr server after installing this version.
