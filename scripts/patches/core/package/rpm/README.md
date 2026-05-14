# RPM package patches

Put shipped patches here when they should apply only to `.rpm` builds.

Use `appliesTo: (context) => context.linuxTarget.packageFormatIs("rpm")`.
