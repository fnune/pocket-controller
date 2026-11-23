{ pkgs, pocket-controller }:
let
  python = pkgs.python3.withPackages (packages: [ packages.evdev ]);
  helpers = pkgs.writeShellScriptBin "vm-helpers" ''
    exec ${python}/bin/python3 ${./vm-helpers.py} "$@"
  '';
  fakePresenter = pkgs.writeShellScriptBin "fake-presenter" (builtins.readFile ./fake-presenter.sh);
in
pkgs.testers.runNixOSTest {
  name = "pocket-controller";
  nodes.machine = {
    environment.systemPackages = [
      pocket-controller
      helpers
      fakePresenter
      pkgs.usbutils
      pkgs.procps
    ];
    virtualisation.memorySize = 1024;
  };
  testScript = builtins.readFile ./vm-test.py;
}
