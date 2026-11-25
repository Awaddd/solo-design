FROM node:22.19.0-bookworm-slim

ARG PI_VERSION=0.83.0
RUN npm install -g "@earendil-works/pi-coding-agent@${PI_VERSION}" \
    && npm cache clean --force

COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod 0755 /usr/local/bin/entrypoint.sh \
    && mkdir -p /solo/outputs && chown node:node /solo/outputs

USER node
WORKDIR /solo/outputs
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
