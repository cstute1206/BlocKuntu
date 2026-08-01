FROM archlinux:latest

# Keep the image limited to the toolchain needed to run makepkg. The PKGBUILD
# declares the project-specific dependencies, which makepkg installs through
# its --syncdeps path during each candidate build.
RUN pacman -Syu --noconfirm --needed base-devel git sudo \
    && useradd --create-home --shell /bin/bash blockuntu-build \
    && printf 'blockuntu-build ALL=(ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/blockuntu-build \
    && chmod 0440 /etc/sudoers.d/blockuntu-build \
    && install -d -o blockuntu-build -g blockuntu-build /work /output /input

USER blockuntu-build
WORKDIR /work
