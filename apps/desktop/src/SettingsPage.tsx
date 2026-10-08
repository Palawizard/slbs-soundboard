import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { communityApi, useCommunityStore } from "./community";
import { communityEnabled, driverEnabled } from "./features";

function message(reason: unknown) { return reason instanceof Error ? reason.message : String(reason); }

export function SettingsPage() {
  const queryClient = useQueryClient();
  const session = useCommunityStore((state) => state.session);
  const setSession = useCommunityStore((state) => state.setSession);
  const [username, setUsername] = useState(session?.user.username ?? "");
  const [notice, setNotice] = useState<string | null>(null);
  const [logs, setLogs] = useState("");
  const diagnostics = useQuery({ queryKey: ["diagnostics-enabled"], queryFn: communityApi.diagnosticsEnabled });
  const driver = useQuery({ queryKey: ["driver-status"], queryFn: communityApi.driverStatus, enabled: driverEnabled });
  const autostart = useQuery({ queryKey: ["autostart-enabled"], queryFn: () => invoke<boolean>("autostart_enabled") });
  const changeAutostart = useMutation({ mutationFn: (enabled: boolean) => invoke("set_autostart", { enabled }), onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: ["autostart-enabled"] }); }, onError: (reason) => setNotice(message(reason)) });
  useEffect(() => setUsername(session?.user.username ?? ""), [session]);

  const login = useMutation({ mutationFn: communityApi.login, onSuccess: (value) => { setSession(value); setNotice("Connexion Google réussie."); }, onError: (reason) => setNotice(message(reason)) });
  const logout = useMutation({ mutationFn: communityApi.logout, onSuccess: () => { setSession(null); setNotice("Vous êtes déconnecté."); queryClient.removeQueries({ queryKey: ["community-owned"] }); }, onError: (reason) => setNotice(message(reason)) });
  const rename = useMutation({ mutationFn: communityApi.updateUsername, onSuccess: (user) => { if (session) setSession({ ...session, user }); setNotice("Le pseudonyme a été enregistré."); }, onError: (reason) => setNotice(message(reason)) });
  const changeDriver = useMutation({ mutationFn: (action: "install" | "remove") => action === "install" ? communityApi.installDriver() : communityApi.removeDriver(), onSuccess: async () => { setNotice("L’opération sur le microphone virtuel est terminée."); await queryClient.invalidateQueries({ queryKey: ["driver-status"] }); }, onError: (reason) => setNotice(message(reason)) });

  return <section className="page settings-page" aria-labelledby="settings-title">
    <header className="page-head"><h1 id="settings-title">Réglages</h1><p>Démarrage, confidentialité{communityEnabled ? " et compte communautaire" : ""}.</p></header>
    {notice && <p className="message" role="status">{notice}</p>}
    {communityEnabled && <section className="panel" aria-labelledby="account-title"><div className="panel-head"><h2 id="account-title">Connexion Google</h2></div>{session ? <><p className="panel-text">Connecté en tant que <strong>{session.user.username}</strong>. Le jeton de session est conservé dans le Gestionnaire d’identifiants Windows.</p><div className="field"><label htmlFor="username">Pseudonyme unique</label><input id="username" value={username} minLength={3} maxLength={32} pattern="[A-Za-z0-9_]+" onChange={(event) => setUsername(event.target.value)} /><p className="field-hint">3 à 32 caractères : lettres, chiffres et tiret bas.</p></div><div className="button-row"><button type="button" className="primary-button" disabled={rename.isPending || !/^[A-Za-z0-9_]{3,32}$/.test(username)} onClick={() => rename.mutate(username)}>Enregistrer</button><button type="button" className="ghost-button danger" disabled={logout.isPending} onClick={() => logout.mutate()}>Se déconnecter</button></div></> : <><p className="panel-text">Connectez-vous pour publier et importer des sons. Le navigateur public reste accessible sans compte.</p><div className="button-row"><button type="button" className="primary-button" disabled={login.isPending} onClick={() => login.mutate()}>{login.isPending ? "Connexion en cours…" : "Continuer avec Google"}</button></div></>}</section>}
    <section className="panel" aria-labelledby="startup-title"><div className="panel-head"><h2 id="startup-title">Lancement et arrière-plan</h2></div><p className="panel-text">Fermer la fenêtre garde l’application et ses raccourcis actifs dans la zone de notification. Utilisez « Quitter » depuis son icône pour arrêter complètement.</p><label className="check-row"><input type="checkbox" checked={autostart.data ?? false} disabled={autostart.isLoading || changeAutostart.isPending} onChange={(event) => changeAutostart.mutate(event.target.checked)} />Lancer au démarrage de Windows</label></section>
    <section className="panel" aria-labelledby="privacy-title"><div className="panel-head"><h2 id="privacy-title">Diagnostics locaux</h2></div><p className="panel-text">Aucune télémétrie n’est envoyée. Ces journaux structurés restent sur cet appareil et ne contiennent ni jeton ni contenu audio.</p><label className="check-row"><input type="checkbox" checked={diagnostics.data ?? true} onChange={(event) => void communityApi.setDiagnosticsEnabled(event.target.checked).then(() => queryClient.invalidateQueries({ queryKey: ["diagnostics-enabled"] }))} />Conserver les diagnostics locaux</label><div className="button-row"><button type="button" className="ghost-button" onClick={() => void communityApi.readDiagnostics().then(setLogs).catch((reason) => setNotice(message(reason)))}>Afficher</button><button type="button" className="ghost-button danger" onClick={() => void communityApi.clearDiagnostics().then(() => { setLogs(""); setNotice("Les diagnostics ont été effacés."); })}>Effacer</button></div>{logs && <pre className="log-output" tabIndex={0}>{logs}</pre>}</section>
    {driverEnabled && <section className="panel" aria-labelledby="driver-title"><div className="panel-head"><h2 id="driver-title">Pilote du microphone virtuel</h2></div><p className="panel-text">{driver.data?.installed ? "Le pilote SLB Virtual Microphone est installé." : driver.data?.packageAvailable ? "Le pilote signé est prêt à être installé." : "Cette version ne contient pas de paquet de pilote signé."}</p><div className="button-row">{driver.data?.installed ? <button type="button" className="ghost-button danger" disabled={changeDriver.isPending} onClick={() => changeDriver.mutate("remove")}>Désinstaller le pilote</button> : <button type="button" className="primary-button" disabled={changeDriver.isPending || !driver.data?.packageAvailable} onClick={() => changeDriver.mutate("install")}>Installer le pilote</button>}<button type="button" className="ghost-button" onClick={() => void driver.refetch()}>Actualiser l’état</button></div></section>}
  </section>;
}
