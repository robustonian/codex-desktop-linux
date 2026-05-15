# DEB package patches

Put shipped patches here when they should apply only to `.deb` builds.

Use `appliesTo: (context) => context.linuxTarget.packageFormatIs("deb")`.
