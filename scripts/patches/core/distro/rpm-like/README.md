# RPM-like core patches

Put shipped patches here when they should apply only to RPM-family systems.

Prefer package-format checks when the packaging format is the real condition:
`appliesTo: (context) => context.linuxTarget.packageFormatIs("rpm")`.
