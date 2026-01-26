FROM alpine AS builder
ARG TARGETARCH
COPY target/x86_64-unknown-linux-musl/release/prefixddns /bin/prefixddns-amd64
COPY target/aarch64-unknown-linux-musl/release/prefixddns /bin/prefixddns-arm64
RUN if [ "$TARGETARCH" = "amd64" ]; then \
      cp /bin/prefixddns-amd64 /bin/prefixddns ; \
    elif [ "$TARGETARCH" = "arm64" ]; then \
      cp /bin/prefixddns-arm64 /bin/prefixddns ; \
    fi

FROM gcr.io/distroless/static
ENV TZ=Asia/Shanghai
COPY --from=builder /bin/prefixddns /bin/prefixddns
VOLUME /data
WORKDIR /data
ENTRYPOINT ["/bin/prefixddns"]
