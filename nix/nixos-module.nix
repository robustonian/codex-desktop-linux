{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.codexDesktopLinux;
  remoteCfg = cfg.remoteControl;
  system = pkgs.stdenv.hostPlatform.system;
  flakePackages = self.packages.${system};
  packageName =
    if cfg.remoteMobileControl.enable && cfg.computerUseUi.enable then
      "codex-desktop-computer-use-ui-remote-mobile-control"
    else if cfg.remoteMobileControl.enable then
      "codex-desktop-remote-mobile-control"
    else if cfg.computerUseUi.enable then
      "codex-desktop-computer-use-ui"
    else
      "codex-desktop";
  desktopPackage = if cfg.package != null then cfg.package else flakePackages.${packageName};
  remoteControlPath = lib.makeSearchPath "bin" (
    [
      "/run/current-system/sw"
    ]
    ++ remoteCfg.extraPackages
  );
  remoteControlEnvironment = {
    CODEX_HOME = if remoteCfg.codexHome != null then remoteCfg.codexHome else "%h/.codex";
    PATH = remoteControlPath;
  }
  // remoteCfg.environment;
  remoteControlEnvironmentList = lib.mapAttrsToList (
    name: value: "${name}=${if lib.isBool value then lib.boolToString value else toString value}"
  ) (lib.filterAttrs (_name: value: value != null) remoteControlEnvironment);
in
{
  options.programs.codexDesktopLinux = {
    enable = lib.mkEnableOption "Codex Desktop for Linux";

    package = lib.mkOption {
      type = lib.types.nullOr lib.types.package;
      default = null;
      defaultText = lib.literalExpression ''
        inputs.codex-desktop-linux.packages.''${pkgs.stdenv.hostPlatform.system}.codex-desktop
      '';
      description = ''
        Codex Desktop package to install. When unset, the module selects one of
        this flake's package variants from
        {option}`programs.codexDesktopLinux.computerUseUi.enable` and
        {option}`programs.codexDesktopLinux.remoteMobileControl.enable`.
      '';
    };

    computerUseUi.enable = lib.mkEnableOption "the Linux Computer Use UI package variant";

    remoteMobileControl.enable = lib.mkEnableOption "the experimental Linux mobile remote-control package variant";

    remoteControl = {
      enable = lib.mkEnableOption "a system-wide user app-server unit with remote control enabled";

      package = lib.mkOption {
        type = lib.types.package;
        default = pkgs.codex;
        defaultText = lib.literalExpression "pkgs.codex";
        description = "Codex CLI package used by the remote-control app-server service.";
      };

      codexHome = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        example = "%h/.codex";
        description = ''
          Value for {env}`CODEX_HOME` in the remote-control service. If unset,
          the global user unit uses {file}`%h/.codex`.
        '';
      };

      listen = lib.mkOption {
        type = lib.types.str;
        default = "unix://";
        description = ''
          Local app-server transport endpoint passed to
          {command}`codex app-server --listen`.
        '';
      };

      target = lib.mkOption {
        type = lib.types.str;
        default = "default.target";
        description = "Systemd user target that starts the remote-control service.";
      };

      environment = lib.mkOption {
        type = lib.types.attrsOf (
          lib.types.nullOr (
            lib.types.oneOf [
              lib.types.bool
              lib.types.int
              lib.types.str
            ]
          )
        );
        default = { };
        description = "Environment variables to set for the remote-control service.";
      };

      environmentFile = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        example = "/run/secrets/codex-remote-control.env";
        description = ''
          Additional environment file as defined in {manpage}`systemd.exec(5)`.
        '';
      };

      extraPackages = lib.mkOption {
        type = lib.types.listOf lib.types.package;
        default = with pkgs; [
          bash
          coreutils
          findutils
          git
          gnugrep
          gnused
          openssh
        ];
        description = "Extra packages to add to {env}`PATH` for commands launched by Codex.";
      };

      extraArgs = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [ ];
        example = [
          "--analytics-default-enabled"
        ];
        description = "Additional arguments passed to {command}`codex app-server`.";
      };

      disableLauncherAutostart = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = ''
          Set {env}`CODEX_REMOTE_CONTROL_DAEMON_AUTOSTART_DISABLED=1` for
          graphical sessions when this declarative service is enabled, so the
          Desktop launcher does not also start the mutable standalone daemon
          hook.
        '';
      };
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = !remoteCfg.enable || pkgs.stdenv.hostPlatform.isLinux;
        message = "`programs.codexDesktopLinux.remoteControl.enable` is only supported on Linux";
      }
    ];

    environment.systemPackages = [
      desktopPackage
    ];

    environment.sessionVariables = lib.mkIf (remoteCfg.enable && remoteCfg.disableLauncherAutostart) {
      CODEX_REMOTE_CONTROL_DAEMON_AUTOSTART_DISABLED = "1";
    };

    systemd.user.services.codex-remote-control = lib.mkIf remoteCfg.enable {
      description = "Codex remote-control app-server";
      after = [ "network.target" ];
      wantedBy = [
        remoteCfg.target
      ];
      serviceConfig = {
        Environment = remoteControlEnvironmentList;
        ExecStart = lib.escapeShellArgs (
          [
            (lib.getExe remoteCfg.package)
            "app-server"
            "--remote-control"
            "--listen"
            remoteCfg.listen
          ]
          ++ remoteCfg.extraArgs
        );
        Restart = "on-failure";
        RestartSec = 5;
      }
      // lib.optionalAttrs (remoteCfg.environmentFile != null) {
        EnvironmentFile = remoteCfg.environmentFile;
      };
    };
  };
}
