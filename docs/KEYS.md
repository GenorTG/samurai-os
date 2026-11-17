# Key runbook — Samurai OS build keys

Spec: `docs/specs/02-PACKAGE-REPOSITORY.md` (Signing keys).
This file is the operational procedure. Keep it in sync with reality.

## Current state (2026-11-17)

| Thing | Value |
|---|---|
| Primary key (certify only, never expires) | `DFE8C148C042AA3C199AAEA08436E12731EDD3FA` |
| Signing subkey (ed25519, sign, expires **2027-11-17**) | on the primary's keyring |
| UID | `Samurai OS Build Key <build@smart-samurai.pl>` |
| Offline keyring location | genorbox1 `~/samurai-keys/gnupg` (0700) |
| Revocation certificate | genorbox1 `~/samurai-keys/gnupg/openpgp-revocs.d/DFE8C148…DD3FA.rev` |
| Fingerprint copy | genorbox1 `~/samurai-keys/primary-fingerprint.txt` |
| Public export (shipped) | `pkgbuilds/samurai-keyring/build-key.asc` (committed to GitHub) |

No passphrase is set yet. The key is low-value (revocable, subkey rotates yearly), but
adding a passphrase to the primary is on the TODO list; do it before the ISO ships.

## Daily use (local builds on genorbox1)

```bash
export GNUPGHOME=~/samurai-keys/gnupg
gpg --detach-sign --local-user <subkey-fpr> FILE       # package/db signing
```

The signing subkey is also exported into the build container per-build and destroyed
with it (`ci/build-repo.sh`); it never lands on disk in the container image.

## GitHub Actions

The signing subkey is stored as a repo secret on `GenorTG/samurai-os`
(`SAMURAI_SIGNING_SUBKEY`, armored) and written to a throwaway GNUPGHOME in CI.
Only the subkey — never the primary. Set it with:

```bash
gh secret set SAMURAI_SIGNING_SUBKEY --repo GenorTG/samurai-os < <(export-armored-subkey)
```

## Backup (TODO: do once, then verify)

1. Copy `~/samurai-keys/` to two USB sticks.
2. Store one off-site.
3. Verify: restore the copy into a scratch GNUPGHOME and sign a test file.

## Rotate the signing subkey (yearly, or on suspicion)

1. New subkey on the offline primary; sign a "new keyring" `samurai-keyring` release
   that contains BOTH keys' public parts; publish and let systems pick it up.
2. Wait one release cycle (spec 02: "publish a new keyring, wait, then sign").
3. Only then switch signing to the new subkey and update the GitHub secret.
4. Old subkey: remove it from the shipped keyring after another cycle. Do **not**
   delete it from the offline primary (it must keep verifying old packages).

## Compromise / full rotation

1. Revoke the primary: publish the revocation cert to keyservers and the keyring.
2. New primary (new fingerprint), new keyring release with both old (revoked) and
   new keys, then stop signing with everything old. Machines verify via
   `SigLevel = Required DatabaseRequired`, so the revoked db stops installing.
3. Post-mortem: find how it leaked, write it up here.

## Owner-trust model

`samurai-keyring` ships `samurai-trusted` with `…DD3FA:4:` (full) — pacman-key
`--populate` imports it. Machines never mark the key ultimate; revocations in
`samurai-revoked` are honored on the next populate.
