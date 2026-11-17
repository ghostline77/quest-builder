# Local playtest controls

Windows developer tool for loading named saves, reloading CET Lua mods and enabling or disabling FreeFly. Requires Cyber Engine Tweaks; FreeFly commands also require FreeFly.

```powershell
cargo build --release --manifest-path tools/playtest/Cargo.toml --target-dir target
$tool = './target/release/ghostline-playtest.exe'
$game = 'C:/Games/Cyberpunk 2077'
$save = Read-Host 'Save folder name'
& $tool --game $game install
& $tool --game $game launch $save
& $tool --game $game list-saves
& $tool --game $game load $save
& $tool --game $game reload
& $tool --game $game freefly on
& $tool --game $game freefly off
& $tool --game $game state
& $tool --game $game quit
```

Install the controller while the game is closed, or reload CET once after installation. Commands report JSON and fail on missing acknowledgements or stale controller heartbeats. Save loading discards unsaved progress. Save names come from the engine's current save list; their numeric suffixes are not load indexes.

`reload` focuses the game, opens CET using its configured binding, clicks the last button in the live CET toolbar and waits for a new controller instance. It closes the overlay afterward. This supports CET's seven-button toolbar layout. It reloads Lua mods; changed archives still require restarting the game.

`load` waits for player attachment after loading. `freefly on` requires an on-foot player and uses FreeFly's own activation function. These commands close CET so gameplay input is available. The controller does not create saves or set quest facts. Do not run competing game-control sessions during these operations.

The installed files live in `bin/x64/plugins/cyber_engine_tweaks/mods/ghostline_playtest`. Keep this developer tool out of distributed quest packages.
