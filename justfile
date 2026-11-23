target := "aarch64-unknown-linux-musl"
release := "target/" + target + "/release"
screen_target := "aarch64-unknown-linux-gnu.2.28"
screen_release := "target/aarch64-unknown-linux-gnu/release"
presenter_version := "0.14.0"
pak := "dist/nextui/Pocket Controller.pak"
stock_app := "dist/trimui-stock/Apps/PocketController"
brick := env("POCKET_CONTROLLER_HOST", "root@192.168.1.92")
ssh_key := env("POCKET_CONTROLLER_SSH_KEY", home_directory() / ".ssh/id_ed25519_brick")
ssh_options := "-i " + ssh_key + " -o BatchMode=yes -o IdentitiesOnly=yes -o LogLevel=ERROR"

default:
    @just --list

check: lint test

lint:
    cargo fmt --all --check
    cargo clippy --all-targets --workspace -- -D warnings

test:
    cargo test --workspace

build:
    cargo build --release --target {{ target }} -p pocket-controller --bins
    cargo zigbuild --release -p pocket-controller-screen --target {{ screen_target }}

pak: build (presenter "tg5040-nextui" "tg5040") (presenter "tg5050-nextui" "tg5050") (presenter "h700-nextui" "h700")
    rm -rf "{{ pak }}"
    mkdir -p "{{ pak }}/bin/arm64" "{{ pak }}/bin/arm64-gnu"
    cp {{ screen_release }}/pocket-controller-screen "{{ pak }}/bin/arm64-gnu/"
    mkdir -p "{{ pak }}/assets"
    find screen/assets -maxdepth 1 -name '*.png' ! -name 'preview-*' -exec cp {} "{{ pak }}/assets/" \;
    cp packaging/nextui/launch.sh packaging/nextui/pak.json "{{ pak }}/"
    cp {{ release }}/pocket-controller "{{ pak }}/bin/arm64/"
    for platform in tg5040 tg5050 h700; do mkdir -p "{{ pak }}/bin/$platform" && cp ".cache/minui-presenter-$platform" "{{ pak }}/bin/$platform/minui-presenter"; done
    chmod +x "{{ pak }}/launch.sh" "{{ pak }}"/bin/*/*
    cd dist/nextui && rm -f Pocket.Controller.pak.zip && cd "Pocket Controller.pak" && zip -qr ../Pocket.Controller.pak.zip .

stock: build (presenter "tg5040" "tg5040-stock")
    mkdir -p "{{ stock_app }}"
    cp packaging/trimui-stock/launch.sh packaging/trimui-stock/config.json {{ release }}/pocket-controller "{{ stock_app }}/"
    cp .cache/minui-presenter-tg5040-stock "{{ stock_app }}/minui-presenter"
    chmod +x "{{ stock_app }}/launch.sh" "{{ stock_app }}/pocket-controller" "{{ stock_app }}/minui-presenter"

presenter asset name:
    #!/usr/bin/env bash
    set -euo pipefail
    declare -A checksums=(
        [tg5040-nextui]=264785d9aa3e5f9ac2074c539e480ea1fd6dbe220ee3a52734511aa9ec759830
        [tg5050-nextui]=7e585c6bd6a96b2715e48029a860e0fe72cd29cb581372cf962f50595275fce8
        [h700-nextui]=bd52303c41ef6a865c9742fe8dc5068b3230725d85ca98e3034beddf2e493fe1
        [tg5040]=772ae8c835185ff113a30b59dd2a764033760ec34aa2dbe632520e1696aec6cf
    )
    file=".cache/minui-presenter-{{ name }}"
    mkdir -p .cache
    if [ ! -f "$file" ]; then
        curl --fail --silent --show-error --location --output "$file.partial" \
            "https://github.com/josegonzalez/minui-presenter/releases/download/{{ presenter_version }}/minui-presenter-{{ asset }}"
        mv "$file.partial" "$file"
    fi
    echo "${checksums[{{ asset }}]}  $file" | sha256sum --check --quiet

deploy: pak
    tar -C "{{ pak }}" -cf - . | ssh {{ ssh_options }} {{ brick }} 'rm -rf "/mnt/SDCARD/Tools/tg5040/Pocket Controller.pak/assets" && mkdir -p "/mnt/SDCARD/Tools/tg5040/Pocket Controller.pak" && tar -C "/mnt/SDCARD/Tools/tg5040/Pocket Controller.pak" -xf -' 
    scp -q {{ ssh_options }} {{ release }}/pocket-controller-inject {{ release }}/pocket-controller-probe {{ brick }}:/tmp/

smoke mode="xbox": build
    scripts/brick-smoke.sh {{ mode }}

e2e: build
    scripts/brick-e2e.py
