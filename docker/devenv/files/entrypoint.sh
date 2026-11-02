#!/usr/bin/env bash

set -e

EMSDK_QUIET=1 . /opt/emsdk/emsdk_env.sh;

usermod -u ${EXTERNAL_UID:-1000} logos;

cp /root/.bashrc /home/logos/.bashrc
cp /root/.vimrc /home/logos/.vimrc
cp /root/.tmux.conf /home/logos/.tmux.conf

chown logos:users /home/logos
rsync -ar --chown=logos:users /opt/cargo/ /home/logos/.cargo/

export PATH="/home/logos/.cargo/bin:$PATH"
export CARGO_HOME="/home/logos/.cargo"

exec "$@"
