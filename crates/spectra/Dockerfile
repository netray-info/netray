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
RUN cargo build --release --bins && cp $(find /build -xdev -name spectra) /

FROM alpine:3.21
RUN apk add --no-cache ca-certificates wget \
 && addgroup -S spectra && adduser -S spectra -G spectra
WORKDIR /spectra
# Operators: copy spectra.example.toml to spectra.toml and edit before deploying
COPY spectra.example.toml /etc/spectra/spectra.example.toml
ENV SPECTRA_SERVER__BIND=0.0.0.0:8082
COPY --from=builder /spectra .
RUN chown -R spectra:spectra /spectra
USER spectra
EXPOSE 8082 9090
CMD ["./spectra"]
