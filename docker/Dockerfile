# Crundi standalone server — Linux.
#
# The server half of Crundi: Claude sessions, terminals, projects, the web UI.
# No Electron, no display. Anything that genuinely needs a GUI (the browser
# panel) is served by a desktop app that attaches to this server and lends it
# one — see the native host bridge in src/browser.js.
#
# Build:  docker build -f docker/Dockerfile -t crundi .
# Run:    see docker/docker-compose.yml
#
# Two stages so the compiler toolchain node-pty needs does not ride along into
# the image that actually runs.

# ─── Stage 1: dependencies ───
FROM node:22-bookworm-slim AS deps

# node-pty builds from source when no prebuild matches the platform.
RUN apt-get update && apt-get install -y --no-install-recommends \
      python3 make g++ ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY package.json package-lock.json* ./
# --omit=dev leaves out electron and electron-builder, which are ~200MB and
# meaningless here: this image never renders anything.
RUN npm ci --omit=dev

# node-screenshots is deliberately left without its Linux binding. It needs
# libdbus/X11 to load and a display to be of any use, and this image has
# neither — pulling a GUI stack into a headless server to satisfy an import
# would be pretending. src/system.js treats a missing capture backend as a
# capability that is absent rather than an error, so the server starts fine and
# only the capture tools report it. Screenshots belong on a machine with a
# screen; run the desktop app there.


# ─── Stage 2: runtime ───
FROM node:22-bookworm-slim

# git: Claude Code reads and writes repositories.
# ripgrep: its search tooling is markedly faster with it present.
# procps: process inspection for the services panel.
RUN apt-get update && apt-get install -y --no-install-recommends \
      git ripgrep procps ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# The Claude Code CLI itself. Crundi spawns this binary; without it the app
# starts but every session fails at launch.
RUN npm install -g @anthropic-ai/claude-code && claude --version

WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY package.json ./
COPY src ./src
COPY app/vendor ./app/vendor
COPY scripts ./scripts
# Logo, favicons and PWA icons. Without these every /assets/* is a 404 and
# the app cannot be installed to a home screen.
COPY assets ./assets

# DATA_DIR and PROJECTS_DIR are volumes so nothing the user cares about lives in
# the container's writable layer, which is discarded on every rebuild.
ENV NODE_ENV=production \
    DATA_DIR=/data \
    PROJECTS_DIR=/projects \
    WEB_PORT=8888
RUN mkdir -p /data /projects

EXPOSE 8888

# Reports unhealthy before the port is even listening, which is what you want
# when the usual failure is a missing TELEGRAM_BOT_TOKEN aborting startup.
HEALTHCHECK --interval=30s --timeout=5s --start-period=25s --retries=3 \
  CMD node -e "fetch('http://127.0.0.1:'+(process.env.WEB_PORT||8888)+'/').then(r=>process.exit(r.status<500?0:1)).catch(()=>process.exit(1))"

CMD ["node", "--no-deprecation", "src/index.js"]
