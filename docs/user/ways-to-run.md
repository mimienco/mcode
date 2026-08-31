# Ways to run MCode

Local desktop, SSH hosts, self-hosted MCode servers, and on-demand per-workspace VMs — pick the right compute for each task.

MCode is not locked to your laptop. Every worktree runs somewhere — on the machine in front of you, on a box you already own, on a shared always-on server, or on a fresh cloud VM spun up for that one workspace.

This page is the map. Deep dives live on the linked pages.

## At a glance

| Mode                                     | Where files and agents live                    | Who owns the machine              | Best for                                      |
| ---------------------------------------- | ---------------------------------------------- | --------------------------------- | --------------------------------------------- |
| **Local**                                | Your desktop                                   | You                               | Day-to-day coding, fast iteration             |
| **SSH target**                           | A remote host you connect to over SSH          | You (or your team)                | Dev boxes, GPU hosts, always-on VPS           |
| **Remote MCode Server**                   | A machine running MCode desktop or `mcode serve` | You (or your team)                | Persistent shared runtime, mobile, automation |
| **Cloud VM / per-workspace environment** | A disposable VM/sandbox per workspace          | Your cloud account (BYO provider) | Isolated, ephemeral agent compute             |

Remote modes always use machines and cloud accounts you control — there is no managed hosting.

## 1. Local desktop

Install MCode, open a project, create worktrees. Agents, terminals, and the browser run on the same machine as the UI.

This is the default path for most people. See [Your first 3-agent session](first-session.md) and [Worktrees](model/worktrees.md).

## 2. SSH targets

Point MCode at any SSH host — a beefier Mac mini, a Linux VPS, a GPU box, a cloud sandbox with SSH. When you create a worktree, pick that host under **Run on**. Agents and `git worktree` run on the remote; the editor, diff, and UI stay on your laptop.

**Good fit when:** the remote already has your repo, tools, and credentials, and you want one laptop MCode to drive many boxes.

**Setup sketch:**

1. Add the host under [Settings → SSH](settings.md).
2. Test the connection (git must be available on the host for repo worktrees).
3. Create a worktree and select the SSH target under **Run on**. The type-ahead lists ready hosts and, for hosts that are connected but not yet set up for this project, non-selectable **setup-needed** rows.

SSH worktrees can open in VS Code Remote-SSH, download remote folders when SFTP allows it, and stay usable for files/git even when the remote cannot compile terminal natives (install build tools for shells). Details: [SSH worktrees](ssh.md) and the recipe [Work on a remote machine over SSH](recipes/remote-worktrees.md).

## 3. Remote MCode Servers

Keep MCode running on a machine you control — an old laptop, Mac mini, home server, cloud VPS, or team box. Pair your laptop, browser client, or phone to that runtime. The **server** owns projects, worktrees, terminals, and agent processes; clients are the UI.

**Good fit when:**

- You want agents to keep running after your laptop sleeps
- Mobile should reconnect to the same sessions
- Automation or a backend should start sessions on a stable host

**Easiest setup:** install MCode and Tailscale on both computers. On the server, open **Settings → Remote MCode Servers → Advertise this app as a server → New Link**, choose its Tailscale address, and generate an access link. On the client, choose **Add Server** and paste that link.

For a headless Linux server or service-managed VM, use `mcode serve` as the alternative:

```
mcode serve --pairing-address <reachable-tailscale-ip-or-hostname>
```

Full detail: [Remote MCode Servers](remote-servers.md).

### SSH vs Remote MCode Server

|               | SSH worktrees                                      | Remote MCode Server                                             |
| ------------- | -------------------------------------------------- | --------------------------------------------------------------- |
| Runtime owner | Laptop MCode                                        | Remote machine (MCode desktop or `mcode serve`)                  |
| Disconnect    | Agents keep running on the host; laptop reattaches | Full session state lives on the server                          |
| Multi-client  | One laptop drives the host                         | Laptop, web, mobile, and automation can share the same runtime  |
| Typical setup | Import SSH config, pick **Run on**                 | Share the server app or run `mcode serve`, then pair with a URL |

## 4. Cloud VMs (per-workspace environments)

Each worktree can boot its own on-demand environment — a cloud sandbox, VM, or local Docker container — from a **recipe** checked into the repo (`mcode.yaml` + lifecycle scripts). Create spins it up; suspend/resume/destroy tear it down. MCode is a thin wrapper: your provider account, images, and billing stay yours.

In the product UI this surface is labeled **Cloud VM** under [Settings → Experimental](settings.md). Recipes still create per-workspace environments.

Providers people wire today include Vercel Sandbox, Fly, Modal, plain SSH hosts, and local Docker. Connection is either **MCode server** (recipe starts `mcode serve` and returns a pairing URL) or **SSH** (recipe returns connection details MCode dials).

**Good fit when:** you want clean isolation per task, disposable compute, or a standard environment every agent boots into.

**Setup sketch:**

1. Enable **Cloud VM** under [Settings → Experimental](settings.md). The pane includes a short **Create a Cloud VM** guide and recipe/runtime controls.
2. Install/update the Cloud VM / per-workspace environment skill if needed.
3. In any workspace, ask your agent:

```
Use the mcode-per-workspace-env skill to set up a per-workspace environment for this repo.
```

4. The skill walks prerequisites → base snapshot → agent auth → `mcode.yaml` recipe → doctor validation.
5. When a recipe appears under **Recipes**, create a worktree and choose it under **Run on**.

Recipes only show up for workspace create once the `environmentRecipes` entry is on the project's **primary** checkout of `mcode.yaml` (not only a feature branch). Doctor and live provision can still run from any branch while you iterate on scripts.

## How to choose

- **Stay local** if the laptop is fast enough and agents are short-lived.
- **SSH** if you already have a VPS or dev box and want agents there without installing a second MCode runtime.
- **Remote MCode Server** if you want one always-on MCode runtime for mobile, browser, and automation.
- **Cloud VM / per-workspace environments** if each task should get a fresh, recipe-defined sandbox that dies with the worktree.

You can mix modes in one install: local worktrees for quick edits, SSH for a GPU box, and a recipe for CI-like isolation.

## Related

- [SSH worktrees](ssh.md)
- [Remote MCode Servers](remote-servers.md)
- [Worktrees](model/worktrees.md)
- [Mobile](mobile.md)
- [Settings reference](settings.md)
