# Source this: the test server's sign-in and chat for the GPUI client
# (written by the seeding step into /tmp/crundi-devtest-<port>/gpui.json).
eval "$(node -e 'const j=require(process.argv[1]);console.log(`export CRUNDI_SERVER=${j.server} CRUNDI_TOKEN=${j.token} CRUNDI_REFRESH=${j.refresh} CRUNDI_CHAT=${j.chat}`)' "${GPUI_SESSION:-/tmp/crundi-devtest-4870/gpui.json}")"
export RUST_LOG="${RUST_LOG:-warn,zbus=off}"
