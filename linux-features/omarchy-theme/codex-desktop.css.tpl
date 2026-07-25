/* Managed by the Codex Desktop Linux omarchy-theme feature. */
:root {
  color-scheme: dark;
  --codex-omarchy-accent: {{ accent }};
  --codex-omarchy-background: {{ background }};
  --codex-omarchy-foreground: {{ foreground }};
  --codex-omarchy-surface: {{ color0 }};
  --codex-omarchy-surface-muted: {{ color8 }};
  --codex-omarchy-selection-background: {{ selection_background }};
  --codex-omarchy-selection-foreground: {{ selection_foreground }};

  --text-primary: {{ foreground }};
  --text-secondary: {{ color7 }};
  --text-tertiary: {{ color8 }};
  --text-quaternary: {{ color8 }};
  --text-error: {{ color1 }};
  --text-success: {{ color2 }};

  --main-surface-primary: {{ background }};
  --main-surface-secondary: {{ color0 }};
  --main-surface-tertiary: {{ color0 }};
  --sidebar-surface-primary: {{ background }};
  --sidebar-surface-secondary: {{ color0 }};
  --sidebar-surface-tertiary: {{ color0 }};
  --composer-surface-primary: {{ background }};
  --composer-surface-secondary: {{ color0 }};

  --border-light: color-mix(in srgb, {{ foreground }} 14%, transparent);
  --border-medium: color-mix(in srgb, {{ foreground }} 24%, transparent);
  --border-heavy: color-mix(in srgb, {{ foreground }} 34%, transparent);
  --accent-primary: {{ accent }};
  --link: {{ accent }};
}

html,
body,
#root {
  background: {{ background }} !important;
  color: {{ foreground }} !important;
}

::selection {
  background: {{ selection_background }} !important;
  color: {{ selection_foreground }} !important;
}

.bg-token-main-surface-primary,
.bg-token-sidebar-surface-primary,
.bg-token-bg-primary {
  background-color: {{ background }} !important;
}

.bg-token-main-surface-secondary,
.bg-token-sidebar-surface-secondary,
.bg-token-bg-secondary,
.bg-token-bg-tertiary {
  background-color: {{ color0 }} !important;
}

.text-token-text-primary {
  color: {{ foreground }} !important;
}

.text-token-text-secondary,
.text-token-text-tertiary {
  color: {{ color7 }} !important;
}

.border-token-border-light,
.border-token-border-medium {
  border-color: color-mix(in srgb, {{ foreground }} 18%, transparent) !important;
}

a,
.text-token-text-link,
.text-token-link {
  color: {{ accent }} !important;
}

.ProseMirror,
textarea,
input {
  caret-color: {{ accent }} !important;
}
