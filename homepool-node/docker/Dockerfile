# The homepool node as a container: app/homepool-node.js on plain Node 22, no packages.
#
# Build from the REPOSITORY ROOT (the file it needs is in app/):
#   docker build -f homepool-node/docker/Dockerfile -t crundi-homepool-node .
# Run with the node code from Crundi (Settings › Homepool › Nodes › Add node):
#   docker run -d --name crundi-homepool-node --restart unless-stopped \
#     -e HOMEPOOL_CODE='crundi-homepool://node?s=…&k=hpn_…' crundi-homepool-node
FROM node:22-alpine

# The script runs its standalone main() only when started under this name
# (homepool-node.mjs, or n.mjs as the server's one-liner saves it).
COPY app/homepool-node.js /app/homepool-node.mjs

# Never root: the node only dials out and needs no privileges.
USER node
ENV NODE_ENV=production
# HOMEPOOL_CODE, or HOMEPOOL_SERVER + HOMEPOOL_KEY; HOMEPOOL_PLATFORM (default "docker").
CMD ["node", "/app/homepool-node.mjs"]
