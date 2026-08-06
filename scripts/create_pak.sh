#!/bin/bash

set -euo pipefail

DIST_DIR="dist"
PAK_DIR_NAME="Updater"
UPDATER_BINARY="target/aarch64-unknown-linux-gnu/release/nextui-updater-rs"
ZIP_FILE="nextui.updater.zip"

rm -rf "${DIST_DIR}"

for PLATFORM in tg5040 tg5050 my355 h700; do
    UPDATER_DIR="${DIST_DIR}/Tools/${PLATFORM}/${PAK_DIR_NAME}.pak"
    mkdir -p "${UPDATER_DIR}"

    cp "${UPDATER_BINARY}" "${UPDATER_DIR}/nextui-updater"
    cp "pak.json" "${UPDATER_DIR}/pak.json"

    LAUNCH_SCRIPT="${UPDATER_DIR}/launch.sh"
    cat > "${LAUNCH_SCRIPT}" <<EOF
#!/bin/sh

cd \$(dirname "\$0")
:> logs.txt

while : ; do

./nextui-updater 2>&1 >> logs.txt

[[ \$? -eq 5 ]] || break

done

EOF
    chmod +x "${LAUNCH_SCRIPT}"
done

#(cd "${DIST_DIR}" && zip -r "../${ZIP_FILE}" .)
for PLATFORM in tg5040 tg5050 my355 h700; do
    (cd "${DIST_DIR}/Tools/${PLATFORM}/${PAK_DIR_NAME}.pak" && zip -r "../../../../${PAK_DIR_NAME}_${PLATFORM}.zip" .)
done
