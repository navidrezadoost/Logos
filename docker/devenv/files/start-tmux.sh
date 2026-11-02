#!/usr/bin/env bash

sudo chown logos:users /home/logos

cd ~;

source ~/.bashrc

echo "[start-tmux.sh] Installing node dependencies"
pushd ~/logos/frontend/
./scripts/setup;
popd
pushd ~/logos/exporter/
./scripts/setup;
popd

tmux -2 new-session -d -s logos

tmux rename-window -t logos:0 'frontend watch'
tmux select-window -t logos:0
tmux send-keys -t logos 'cd logos/frontend' enter C-l
tmux send-keys -t logos './scripts/watch app' enter

tmux new-window -t logos:1 -n 'frontend storybook'
tmux select-window -t logos:1
tmux send-keys -t logos 'cd logos/frontend' enter C-l
tmux send-keys -t logos './scripts/watch storybook' enter

tmux new-window -t logos:2 -n 'exporter'
tmux select-window -t logos:2
tmux send-keys -t logos 'cd logos/exporter' enter C-l
tmux send-keys -t logos 'rm -f target/app.js*' enter C-l
tmux send-keys -t logos './scripts/watch' enter

tmux split-window -v
tmux send-keys -t logos 'cd logos/exporter' enter C-l
tmux send-keys -t logos './scripts/wait-and-start.sh' enter

tmux new-window -t logos:3 -n 'backend'
tmux select-window -t logos:3
tmux send-keys -t logos 'cd logos/backend' enter C-l
tmux send-keys -t logos './scripts/start-dev' enter

tmux -2 attach-session -t logos
