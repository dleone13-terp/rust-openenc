FROM mcr.microsoft.com/devcontainers/rust:2-1-bookworm

# Include lld linker to improve build times either by using environment variable
# RUSTFLAGS="-C link-arg=-fuse-ld=lld" or with Cargo's configuration file (i.e see .cargo/config.toml).
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
   && apt-get -y install clang lld \
   && apt-get autoremove -y && apt-get clean -y

# Build GDAL C++ core from source
ARG GDAL_VERSION=3.12.2
RUN apt-get update && apt-get install -y \
   build-essential \
   wget \
   git \
   cmake \
   libproj-dev \
   libsqlite3-dev \
   libtiff-dev \
   libgeos-dev \
   libexpat1-dev \
   libjpeg-dev \
   libpng-dev \
   libzstd-dev \
   liblzma-dev \
   libcurl4-openssl-dev \
   libxml2-dev \
   && rm -rf /var/lib/apt/lists/*

# Install sqlx-cli using cargo
RUN cargo install sqlx-cli --no-default-features --features postgres

RUN wget https://github.com/OSGeo/gdal/archive/refs/tags/v${GDAL_VERSION}.tar.gz -O /tmp/gdal.tar.gz \
   && mkdir -p /tmp/gdal-src \
   && tar -xzf /tmp/gdal.tar.gz -C /tmp/gdal-src --strip-components=1 \
   && cd /tmp/gdal-src \
   && mkdir build \
   && cd build \
   && cmake .. -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON -DBUILD_TESTING=OFF -DGDAL_BUILD_OPTIONAL_DRIVERS=ON -DGDAL_ENABLE_DRIVER_GTIFF=ON -DGDAL_ENABLE_DRIVER_PNG=ON -DGDAL_ENABLE_DRIVER_JPEG=ON \
   && make -j$(nproc) \
   && make install \
   && ldconfig \
   && cd / \
   && rm -rf /tmp/gdal-src /tmp/gdal.tar.gz