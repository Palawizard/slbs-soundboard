FROM node:22-bookworm-slim AS build
WORKDIR /app
COPY package.json package-lock.json ./
COPY apps/api/package.json apps/api/package.json
COPY apps/desktop/package.json apps/desktop/package.json
COPY packages/contracts/package.json packages/contracts/package.json
RUN npm ci
COPY apps/api apps/api
COPY packages/contracts packages/contracts
RUN npm run build --workspace @slb/contracts && npm run build --workspace @slb/api && npm prune --omit=dev

FROM node:22-bookworm-slim AS runtime
ENV NODE_ENV=production
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl ffmpeg && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --chown=node:node --from=build /app/package.json /app/package-lock.json ./
COPY --chown=node:node --from=build /app/node_modules node_modules
COPY --chown=node:node --from=build /app/apps/api/package.json apps/api/package.json
COPY --chown=node:node --from=build /app/apps/api/dist apps/api/dist
COPY --chown=node:node --from=build /app/apps/api/migrations apps/api/migrations
COPY --chown=node:node --from=build /app/packages/contracts/package.json packages/contracts/package.json
COPY --chown=node:node --from=build /app/packages/contracts/src packages/contracts/src
COPY --chown=node:node --from=build /app/packages/contracts/dist packages/contracts/dist
RUN mkdir -p /data/media && chown -R node:node /data
USER node
EXPOSE 3000
HEALTHCHECK --interval=10s --timeout=3s --start-period=15s --retries=6 CMD curl --fail --silent http://127.0.0.1:3000/health || exit 1
CMD ["node", "apps/api/dist/start.js"]
