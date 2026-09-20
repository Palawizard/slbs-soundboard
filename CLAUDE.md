# SLB's Soundboard

La documentation de travail du projet vit dans `.cursor/`, qui n'est pas
commité. **Lis ces trois fichiers avant toute modification :**

- `.cursor/docs/project.md` — intention, limites produit, stack, architecture,
  domaine métier et décisions structurantes.
- `.cursor/docs/agents.md` — conventions, état courant, avancement par epic,
  vérifications passées et pièges connus.
- `.cursor/plan.md` — le suivi des tâches, à tenir à jour.

Si `.cursor/` est absent de ta copie, demande-le plutôt que de deviner les
conventions.

## Le minimum à savoir tout de suite

- Code, identifiants et commentaires en anglais. Textes d'interface en français,
  simples et non techniques. Pas d'emoji.
- Commits `type(scope): thing done`, sujet seul.
- Branches `type/thing-done`, une par epic, mergées dans `dev` après revue.
- Le thread audio temps réel n'alloue pas, ne touche ni disque, ni réseau, ni
  base, ne journalise pas et n'attend jamais sur un verrou non borné.
- Les soundboards locaux doivent fonctionner sans compte et sans réseau.
