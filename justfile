target := "aarch64-unknown-linux-musl"
release := "target/" + target + "/release"
screen_target := "aarch64-unknown-linux-gnu.2.28"
screen_release := "target/aarch64-unknown-linux-gnu/release"
presenter_version := "0.14.0"
nextui_font_commit := "be751f7812bcfc1181b2f0da285bfcb02a5d57c3"
nextui_font_sha256 := "b653557e79afa5c63ac5fd2a8c4e16c1a55c5addb6cbaf767b4bf4e57c144056"
pak := "dist/nextui/Pocket Controller.pak"
stock_app := "dist/trimui-stock/Apps/PocketController"
brick := env("POCKET_CONTROLLER_HOST", "root@192.168.1.92")
ssh_key := env("POCKET_CONTROLLER_SSH_KEY", home_directory() / ".ssh/id_ed25519_brick")
ssh_options := "-i " + ssh_key + " -o BatchMode=yes -o IdentitiesOnly=yes -o LogLevel=ERROR -o ConnectTimeout=5 -o ServerAliveInterval=5 -o ServerAliveCountMax=3"

default:
    @just --list

check: lint test assets-check

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
    cp screen/assets/*.png "{{ pak }}/assets/"
    cp -r screen/assets/devices "{{ pak }}/assets/"
    cp packaging/nextui/launch.sh pak.json "{{ pak }}/"
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

screenshots: nextui-font
    cargo build --release -p pocket-controller-screen
    scripts/screenshots.sh .cache/nextui-font1.ttf

nextui-font:
    #!/usr/bin/env bash
    set -euo pipefail
    file=.cache/nextui-font1.ttf
    mkdir -p .cache
    if [ ! -f "$file" ]; then
        curl --fail --silent --show-error --location --output "$file.partial" \
            "https://raw.githubusercontent.com/LoveRetro/NextUI/{{ nextui_font_commit }}/skeleton/SYSTEM/res/font1.ttf"
        mv "$file.partial" "$file"
    fi
    echo "{{ nextui_font_sha256 }}  $file" | sha256sum --check --quiet

assets:
    scripts/assets.py

assets-check:
    scripts/assets.py --check

release-files: pak stock
    rm -rf dist/release
    mkdir -p dist/release
    cp dist/nextui/Pocket.Controller.pak.zip dist/release/
    cd dist/trimui-stock && zip -qr ../release/PocketController-trimui-stock.zip Apps

release-create version:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "$(jj diff --summary)" ]; then
        echo "The working copy has changes. Commit them first."
        exit 1
    fi
    jj git fetch
    if [ "$(jj log --no-graph -r 'main' -T commit_id)" != "$(jj log --no-graph -r 'main@origin' -T commit_id)" ]; then
        echo "Local main differs from origin. Push or rebase first."
        exit 1
    fi
    jj new main
    sed -i '0,/^version = ".*"/s//version = "{{ version }}"/' Cargo.toml screen/Cargo.toml
    cargo update --workspace --quiet
    jq --indent 2 '.version = "v{{ version }}"' pak.json > pak.json.tmp
    mv pak.json.tmp pak.json
    git-cliff --tag "v{{ version }}" --unreleased --prepend CHANGELOG.md
    awk 'prev != "" && /^## \[/ {print ""} {print; prev = $0}' CHANGELOG.md > CHANGELOG.md.tmp
    mv CHANGELOG.md.tmp CHANGELOG.md
    jj commit -m "chore(release): release v{{ version }}"
    jj bookmark set main -r @-
    jj git push --bookmark main
    git tag "v{{ version }}" "$(jj log --no-graph -r main -T commit_id)"
    git push origin "v{{ version }}"

release-delete version:
    gh release delete "v{{ version }}" --repo fnune/pocket-controller --yes || true
    git tag -d "v{{ version }}" || true
    git push origin --delete "v{{ version }}" || true
