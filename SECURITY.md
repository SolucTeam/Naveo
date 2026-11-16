# Politique de sécurité

## Signaler une vulnérabilité

**Ne pas ouvrir d'issue publique pour une vulnérabilité.**

Signale-la en privé :
- via GitHub : onglet *Security* → *Report a vulnerability* (conseillé), ou
- par email à l'adresse du mainteneur du dépôt.

Réponse visée sous **72 h**, avec une évaluation et un calendrier de correction.

## Périmètre

- `naveo-core` : tout ce qui touche aux clients Docker/Kubernetes (injection, fuite de credentials, SSRF…)
- `naveo` : l'UI (exécution de commandes non prévue, affichage de secrets…)

Hors périmètre : les vulnérabilités des dépendances sans impact démontré sur
Naveo — signale-les plutôt en amont, puis tiens-nous au courant.
