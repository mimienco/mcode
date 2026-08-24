# Development and production lanes

MCode has two intentionally separate local runtime lanes.

## Development

Run:

```sh
pnpm app:dev
```

This starts the hot-reload Electron app with the MCode Dev name and DEV icon. Every launch gets a new profile under the operating system's temporary directory. The profile is removed after a normal exit, so repositories, tabs, caches, runtime discovery files, and settings cannot leak into the next dev launch or into production.

For migration or persistence testing only, retain the shared `mcode-dev` profile with:

```sh
pnpm app:dev:persistent
```

## Production

Build the packaged production app for the current platform:

```sh
pnpm app:prod:build
```

Production artifacts are written beneath `dist/prod`, separately from other packaging output. Launch the last successful production build with:

```sh
pnpm app:prod
```

The launcher refuses to run Electron Preview or legacy artifacts. The packaged app keeps the MCode identity and its durable operating-system userData directory. Packaging and updates replace application files without deleting that profile.
