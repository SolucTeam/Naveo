# Contribuer à Naveo

Merci de ton intérêt ! Toute contribution est bienvenue : bug, feature, doc, traduction…

## Démarrage rapide

```sh
git clone https://github.com/crabearaignee/Naveo
cd Naveo
cargo run            # build debug + lancement
cargo clippy         # lint (obligatoire : zéro warning en CI)
cargo fmt            # format (obligatoire : rustfmt en CI)
```

Prérequis : [Rust stable](https://rustup.rs). Sous Linux, les paquets de dev
X11/Wayland sont nécessaires (voir `.github/workflows/ci.yml`).

## Avant d'ouvrir une PR

- [ ] `cargo fmt --all -- --check` passe
- [ ] `cargo clippy --all-targets -- -D warnings` passe
- [ ] `cargo build` passe
- [ ] Une issue existe pour le changement (ou ouvre-en une d'abord pour les gros changements)

## Style

- Commits : [Conventional Commits](https://www.conventionalcommits.org/fr/) (`feat:`, `fix:`, `docs:`, `chore:`…)
- Une PR = un sujet
- L'UI suit la palette définie dans `crates/naveo/src/theme.rs` — pas de couleurs en dur dans les vues

## Architecture en bref

```
UI (egui) ──Command──► tâche tokio (naveo-core) ──► bollard / kube-rs
     ▲                       │
     └────Event (canal) ◄────┘
```

- L'UI ne fait **jamais** d'appel réseau direct : elle envoie des `Command`
  et consomme des `Event`.
- Toute donnée affichée passe par les structs de `crates/naveo-core/src/events.rs`.

## Signaler un bug

Ouvre une issue avec le template *Bug report* : OS, version, étapes de
reproduction, logs. Merci !
