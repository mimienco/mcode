# Remote MCode Servers

Keep MCode running on another computer and connect from your laptop.

A Remote MCode Server lets one computer do the work while another computer provides the UI. The server keeps the projects, worktrees, terminals, tabs, provider accounts, and agent sessions. Your laptop connects to that running MCode instance.

The easiest setup is the MCode desktop app on both computers, connected through [Tailscale](https://tailscale.com/). You do not need to run `mcode serve` for this path.

## What runs where

```
Client computer · MCode client
  Shows the UI and sends your input
                 │
                 │ private network route
                 ▼
Server computer · MCode server
  Stores repos and worktrees
  Runs terminals and agents
```

Install and authenticate Codex, Claude Code, OpenCode, `git`, and any provider CLIs on the **server computer**. A login on your laptop does not automatically carry over to the server.

On a headless `mcode serve` host, register managed Claude/Codex accounts from the server shell (the remote client disables **Add account**):

```
mcode account add --agent claude
mcode account add --agent codex
mcode account list
```

Install or refresh agent skills without a Settings UI:

```
mcode skills install --skill mcode-cli --skill orchestration
mcode skills update --all
```

## Recommended: desktop app + Tailscale

You need:

- MCode installed and updated on both computers
- Tailscale installed on both computers
- Both computers signed in to the same tailnet
- The server computer awake, online, and running MCode

Tailscale gives the server a private address that usually starts with `100.`. MCode puts that address first in its connection-address picker.

### 1. Create an access link on the server

On the computer that should keep the sessions running:

1. Open the MCode desktop app.
2. Open **Settings → Remote MCode Servers**.
3. Under **Advertise this app as a server**, click **New Link**.
4. For **Connection address**, select the Tailscale address. It usually looks like `100.x.y.z`.
5. Click **Generate Access Link**.
6. Copy the link under **Pair another MCode client**.

If the Tailscale address is missing, confirm Tailscale is connected and click the refresh button beside **Connection address**. The grant can be revoked later.

### 2. Add the server on your laptop

On the computer you want to use as the client:

1. Open **Settings → Remote MCode Servers**.
2. Click **Add Server**.
3. Enter a recognizable name, such as `Remote Server`.
4. Paste the access link from the server.
5. Click **Add Server**.
6. If the saved server shows **Disconnected**, click **Connect**.

Adding a server saves it without forcing every new project onto it. Open **Advanced → Active Server** only when you want server-routed projects, terminals, provider checks, and browser or mobile handoff to use that server by default.

## Use the remote server

Once the server shows **Connected**, select it or one of its projects and use MCode normally. Terminals, agent processes, files, worktrees, and session state live on the server computer.

When several paired clients share that server, the sidebar filter can offer **Hide other-client workspaces** so this device only lists workspaces it created. See [Worktrees → Sidebar layout](model/worktrees.md#sidebar-layout).

That means:

- agents keep running when the client laptop sleeps or disconnects;
- the server needs the repository, tools, and credentials used by those agents;
- the server must stay awake and connected to the tailnet;
- reconnecting the client or reopening MCode returns you to the server-owned workspace, tab, and visible-pane state without duplicating paired tabs;
- projects you delete on the server disappear from every paired client's sidebar (no permanent ghost rows).

## Access and security

MCode creates a separate, revocable token for each paired client. The server lists these under **Shared Server Access**.

- Click the trash button beside a grant to revoke it. Active clients using that grant are disconnected immediately.
- Generating another link replaces the previous **unused** link. Clients that already paired keep their own grants until you revoke them.
- Keep Tailscale ACLs or grants as narrow as your setup allows.
- Do not forward the MCode port directly to the public internet. Prefer Tailscale, WireGuard, a trusted LAN, SSH forwarding, or an authenticated tunnel.
- Do not select `127.0.0.1` for another computer. That address only works on the server itself.

## Alternative: `mcode serve`

Use `mcode serve` when the host should run without the desktop window — for example, a headless Linux server or a service-managed VM. For a MacBook or desktop you can leave signed in, the in-app setup above is simpler.

Install MCode and its bundled CLI on the server, then run:

```
mcode serve --pairing-address <server-tailscale-ip-or-hostname>
```

For example:

```
mcode serve --pairing-address 100.64.1.20
```

The command:

- starts the MCode runtime without opening the desktop window;
- runs in the foreground until you press `Ctrl-C`;
- prints the bound endpoint and a runtime pairing URL;
- uses `--pairing-address` only for the address clients should dial.

Paste the printed pairing URL into **Settings → Remote MCode Servers → Add Server** on the client.

Add `--port 6768` when a firewall, tunnel, or service definition requires a fixed port:

```
mcode serve --port 6768 --pairing-address 100.64.1.20
```

Use only one host mode at a time. If the MCode desktop app is already sharing that computer, do not start a second `mcode serve` process for the same setup.

### Mobile from a headless server

For the MCode mobile app, request a mobile-scoped QR code and link:

```
mcode serve --pairing-address 100.64.1.20 --mobile-pairing
```

Keep the phone on the same tailnet, open MCode Mobile, choose **Pair**, and scan the terminal QR code or paste the printed link.

## Desktop app or `mcode serve`?

|               | Desktop app on the server           | `mcode serve`                                       |
| ------------- | ----------------------------------- | --------------------------------------------------- |
| Best for      | An old laptop, Mac mini, or desktop | A headless Linux box, VM, or managed service        |
| Setup         | Settings and buttons                | Terminal command and service configuration          |
| Server window | Open                                | None                                                |
| Access link   | **New Link → Generate Access Link** | Printed in the terminal                             |
| Lifetime      | While the desktop app is running    | While the foreground process or service is running  |

## Remote MCode Server or SSH?

- Use [SSH worktrees](ssh.md) when MCode on your laptop should own the runtime and use another machine only to run selected worktrees and terminals.
- Use a Remote MCode Server when the other machine should own the full MCode runtime and preserve shared sessions for desktop, browser, mobile, or automation clients.

See [Ways to run MCode](ways-to-run.md) for the full comparison.

## Troubleshooting

### The Tailscale address is not listed

On the server, confirm Tailscale is connected, then click the refresh button beside **Connection address**. Both computers must be signed in to the same tailnet. A Tailscale IPv4 address usually starts with `100.`.

### The server is disconnected

Confirm the server computer is awake, MCode is still running, and Tailscale shows both devices online. Check your tailnet ACLs or grants if one device cannot reach the other.

Update MCode on both computers if the server row reports an incompatible protocol version.

### The access link was shared with the wrong person

On the server, open **Settings → Remote MCode Servers → Shared Server Access** and revoke that grant. Generate a new link for the intended client.

### The server cannot find an agent CLI

Install and authenticate that CLI on the server computer. Remote sessions use the server's `PATH`, home directory, and credentials — not the client's.

### `mcode serve` advertises the wrong address

Stop the command and restart it with an address the client can reach:

```
mcode serve --pairing-address <reachable-tailscale-ip-or-hostname>
```

Do not use a wildcard address or `127.0.0.1` for a remote client.

## Related

- Compare every run mode in [Ways to run MCode](ways-to-run.md).
- Use [SSH worktrees](ssh.md) when the laptop should own the MCode runtime.
- See the [MCode CLI reference](cli/reference.md) for `mcode serve` flags and automation commands.
