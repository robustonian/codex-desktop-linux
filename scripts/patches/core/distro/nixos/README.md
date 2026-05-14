# NixOS-specific core patches

Put shipped patches here when they should apply only to NixOS.

Each patch lives in a `patch.js` descriptor and should self-filter with
`appliesTo: (context) => context.linuxTarget.matchesId("nixos")`.
