# Dockerfile
FROM alpine:latest
LABEL org.opencontainers.image.source=https://github.com/gi-z/throwie-server-rust

# Set the working directory
WORKDIR /app

# Copy the downloaded executable to the image
COPY ./throwie-server /app/throwie-server

# Give execution permissions to the binary
RUN chmod +x /app/throwie-server

# Set the entrypoint to run the executable
ENTRYPOINT ["/app/throwie-server"]
