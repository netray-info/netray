FROM node:22-alpine AS frontend
WORKDIR /build/frontend
COPY frontend/package.json frontend/package-lock.json frontend/.npmrc ./
RUN --mount=type=secret,id=NODE_AUTH_TOKEN,env=NODE_AUTH_TOKEN npm ci
COPY frontend/ .
RUN npm run build

FROM clux/muslrust:stable AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock build.rs ./
COPY src src/
COPY --from=frontend /build/frontend/dist frontend/dist/
RUN cargo build --release --bins && cp $(find /build -xdev -name beacon) /

FROM alpine:3.21
RUN apk add --no-cache ca-certificates wget \
 && addgroup -S beacon && adduser -S beacon -G beacon
WORKDIR /beacon
COPY beacon.toml beacon.toml
ENV BEACON__SERVER__BIND=0.0.0.0:3000
COPY --from=builder /beacon .
RUN chown -R beacon:beacon /beacon
USER beacon
EXPOSE 3000 9090
CMD ["./beacon", "beacon.toml"]
