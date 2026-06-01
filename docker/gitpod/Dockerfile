FROM gitpod/workspace-postgres

# Logos devenv for Gitpod.
# Stack: Go backend + Rust/WASM engine + TypeScript/React frontend.
# No JVM. No Clojure.

RUN set -ex; \
    brew install redis; \
    brew install imagemagick; \
    brew install ghostscript; \
    brew install mailhog; \
    brew install openldap; \
    brew install poppler; \
    sudo mkdir -p /var/log/nginx; \
    sudo chown gitpod:gitpod /var/log/nginx

COPY files/nginx.conf /etc/nginx/nginx.conf
COPY files/nginx-mime.types /etc/nginx/mime.types

USER root

ENV NODE_VERSION=v22.22.0 \
    GO_VERSION=1.23.8 \
    LANG=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8

# Browser / font dependencies for Playwright
RUN set -ex; \
    apt-get -qq update; \
    apt-get -qqy install \
        libasound2 \
        libatk1.0-0 \
        libatk-bridge2.0-0 \
        libcairo2 \
        libcups2 \
        libdbus-1-3 \
        libexpat1 \
        libfontconfig1 \
        libgcc1 \
        libglib2.0-0 \
        libgtk-3-0 \
        libnspr4 \
        libpango-1.0-0 \
        libpangocairo-1.0-0 \
        libx11-6 \
        libx11-xcb1 \
        libxcb1 \
        libxcomposite1 \
        libxcursor1 \
        libxdamage1 \
        libxext6 \
        libxfixes3 \
        libxi6 \
        libxrandr2 \
        libxrender1 \
        libxshmfence1 \
        libxss1 \
        libxtst6 \
        fonts-liberation \
        libnss3 \
        libgbm1 \
    ; \
    rm -rf /var/lib/apt/lists/*;

# Go toolchain
RUN set -ex; \
    curl -LfsSo /tmp/go.tar.gz "https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz"; \
    tar -C /usr/local -xf /tmp/go.tar.gz; \
    rm -rf /tmp/go.tar.gz;

USER gitpod

ENV PATH="/usr/local/go/bin:$PATH" \
    GOROOT=/usr/local/go \
    LOGOS_SMTP_ENABLED=true \
    LOGOS_SMTP_HOST=localhost \
    LOGOS_SMTP_PORT=1025 \
    LOGOS_SMTP_SSL=false \
    LOGOS_SMTP_TLS=false \
    LOGOS_SMTP_DEFAULT_REPLY_TO=no-reply@example.com \
    LOGOS_SMTP_DEFAULT_FROM=no-reply@example.com \
    DATABASE_URL="postgres://localhost/logos" \
    REDIS_URL="redis://localhost/0"
