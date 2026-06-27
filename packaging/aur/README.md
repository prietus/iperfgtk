# AUR packaging

Two PKGBUILDs for Arch Linux:

- **`iperfgtk-git`** — builds from the latest git `HEAD`.
- **`iperfgtk-bin`** — installs the prebuilt binaries from the GitHub Release
  matching `pkgver` (produced by `.github/workflows/build.yml`).

Both share the base name `iperfgtk` via `provides`/`conflicts`, so only one can
be installed at a time.

## Test a build locally (on Arch)

```sh
cd packaging/aur/iperfgtk-git   # or iperfgtk-bin
makepkg -si
```

## Publish to the AUR

Each package is its own AUR git repo. For each one:

```sh
# 1) For -bin: bump pkgver to the released tag, then fill in the hashes:
updpkgsums

# 2) Generate the metadata file the AUR requires:
makepkg --printsrcinfo > .SRCINFO

# 3) Commit PKGBUILD + .SRCINFO to ssh://aur@aur.archlinux.org/iperfgtk-{git,bin}.git
```

Notes:
- `iperfgtk-bin` only works once a `vX.Y.Z` **Release with the binary assets
  exists** (tag the repo to trigger the release job in CI).
- `iperfgtk-git`'s `pkgver` is computed automatically from `git describe`; the
  placeholder value in the file is just so it parses before the first build.
