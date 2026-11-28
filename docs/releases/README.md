# Release records

`ci/release-iso.sh --publish` refuses to run without `--verified FILE`: a markdown body (no heading)
saying what was tested for **that** release and what was not. Keep it here as
`<date>-<tag>-verified.md`, written from this release's test runs, never copied from an older one.
`--new FILE` adds the "New in this release" section the same way. The `Release ISO` workflow takes
both paths as inputs.
