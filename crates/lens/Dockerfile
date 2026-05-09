FROM node:22-alpine AS frontend
WORKDIR /build/frontend
COPY frontend/package.json frontend/package-lock.json frontend/.npmrc ./
RUN --mount=type=cache,target=/root/.npm \
    --mount=type=secret,id=NODE_AUTH_TOKEN,env=NODE_AUTH_TOKEN \
    npm ci
COPY frontend/ .
RUN npm run build

FROM clux/muslrust:stable AS builder
WORKDIR /build
# `.dockerignore` prunes target/, node_modules/, .git/, etc. so adding new
# top-level resource directories (assets/, migrations/, …) requires no
# Dockerfile change.
COPY . .
COPY --from=frontend /build/frontend/dist frontend/dist/
RUN cargo build --release --bins && cp $(find /build -xdev -name lens) /

FROM alpine:3.21
RUN apk add --no-cache ca-certificates wget \
 && addgroup -S lens && adduser -S lens -G lens
WORKDIR /lens
COPY lens.example.toml lens.toml
ENV LENS_SERVER__BIND=0.0.0.0:8082
COPY --from=builder /lens .
RUN chown -R lens:lens /lens
USER lens
EXPOSE 8082 8090
# To override the baked-in config, mount your file to /lens/lens.toml:
#   volumes: ["./lens.toml:/lens/lens.toml:ro"]
CMD ["./lens", "lens.toml"]
