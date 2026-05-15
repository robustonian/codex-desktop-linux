# Debian-like core patches

Put shipped patches here when they should apply only to Debian-family systems.

Prefer package-format checks when the packaging format is the real condition:
`appliesTo: (context) => context.linuxTarget.packageFormatIs("deb")`.
