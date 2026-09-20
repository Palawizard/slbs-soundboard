import { useEffect, useMemo, useRef, useState } from "react";
import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { communityApi, formatBytes, useCommunityStore, type CommunitySound } from "./community";
import type { LibrarySnapshot, Sound } from "./library";

function useDebounced(value: string, delay = 300) {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => { const timer = window.setTimeout(() => setDebounced(value), delay); return () => window.clearTimeout(timer); }, [delay, value]);
  return debounced;
}

function message(reason: unknown) { return reason instanceof Error ? reason.message : String(reason); }

export function CommunityPage() {
  const queryClient = useQueryClient();
  const session = useCommunityStore((state) => state.session);
  const previewId = useCommunityStore((state) => state.previewId);
  const setPreview = useCommunityStore((state) => state.setPreview);
  const progress = useCommunityStore((state) => state.progress);
  const setProgress = useCommunityStore((state) => state.setProgress);
  const [search, setSearch] = useState("");
  const [selectedBoard, setSelectedBoard] = useState("");
  const [descriptions, setDescriptions] = useState<Record<string, string>>({});
  const [notice, setNotice] = useState<string | null>(null);
  const audioRef = useRef<HTMLAudioElement>(null);
  const debouncedSearch = useDebounced(search);

  const apiUrl = useQuery({ queryKey: ["community-api-url"], queryFn: communityApi.apiUrl, staleTime: Infinity });
  const library = useQuery({ queryKey: ["library", "community"], queryFn: () => invoke<LibrarySnapshot>("library_snapshot") });
  const publications = useInfiniteQuery({
    queryKey: ["community-publications", debouncedSearch],
    queryFn: ({ pageParam, signal }) => communityApi.browse(debouncedSearch, pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.nextCursor,
    staleTime: 5 * 60_000,
    gcTime: 30 * 60_000,
    retry: 3,
    retryDelay: (attempt) => Math.min(1000 * 2 ** attempt, 8000),
  });
  const owned = useQuery({ queryKey: ["community-owned"], queryFn: communityApi.owned, enabled: Boolean(session), staleTime: 60_000 });

  useEffect(() => { const first = library.data?.soundboards[0]?.id; if (!selectedBoard && first) setSelectedBoard(first); }, [library.data, selectedBoard]);
  useEffect(() => () => { audioRef.current?.pause(); }, []);
  const items = useMemo(() => publications.data?.pages.flatMap((page) => page.items) ?? [], [publications.data]);
  const sounds = library.data?.soundboards.flatMap((board) => board.sounds) ?? [];
  const ownedIds = new Set(owned.data?.map((item) => item.id));

  const publish = useMutation({
    mutationFn: ({ sound, description }: { sound: Sound; description: string }) => communityApi.publish(sound.id, description, (value) => setProgress(sound.id, value)),
    onSuccess: async () => { setNotice("Le son est maintenant publié."); await Promise.all([queryClient.invalidateQueries({ queryKey: ["community-owned"] }), queryClient.invalidateQueries({ queryKey: ["library"] }), queryClient.invalidateQueries({ queryKey: ["community-publications"] })]); },
    onError: (reason) => setNotice(message(reason)),
    onSettled: (_data, _error, variables) => window.setTimeout(() => setProgress(variables.sound.id, null), 1200),
  });
  const importSound = useMutation({
    mutationFn: ({ publication, soundboardId }: { publication: CommunitySound; soundboardId: string }) => communityApi.import(publication.id, soundboardId, (value) => setProgress(publication.id, value)),
    onSuccess: async () => { setNotice("Le son est disponible hors ligne dans votre soundboard."); await queryClient.invalidateQueries({ queryKey: ["library"] }); },
    onError: (reason) => setNotice(message(reason)),
    onSettled: (_data, _error, variables) => window.setTimeout(() => setProgress(variables.publication.id, null), 1200),
  });
  const remove = useMutation({
    mutationFn: communityApi.remove,
    onSuccess: async () => { setNotice("La publication a été retirée."); await Promise.all([queryClient.invalidateQueries({ queryKey: ["community-owned"] }), queryClient.invalidateQueries({ queryKey: ["library"] }), queryClient.invalidateQueries({ queryKey: ["community-publications"] })]); },
    onError: (reason) => setNotice(message(reason)),
  });

  function togglePreview(id: string) {
    if (previewId === id) { audioRef.current?.pause(); setPreview(null); return; }
    audioRef.current?.pause();
    setPreview(id);
    window.setTimeout(() => void audioRef.current?.play().catch(() => setNotice("L’aperçu audio n’a pas pu démarrer.")), 0);
  }

  return <section className="workspace community-page">
    <header className="topbar"><div><p className="eyebrow">Partage</p><h1>Communauté</h1></div><span className={`status ${session ? "status-running" : "status-stopped"}`}><span className="status-dot" />{session ? `Connecté · ${session.user.username}` : "Navigation publique"}</span></header>
    {(publications.isError || owned.isError) && <div className="outage-banner" role="alert"><strong>Service communautaire indisponible.</strong><span>La bibliothèque locale reste utilisable. Vérifiez Internet ou le tunnel, puis réessayez.</span><button type="button" onClick={() => void publications.refetch()}>Réessayer</button></div>}
    {notice && <p className="notice" role="status">{notice}</p>}
    <section className="community-toolbar" aria-label="Recherche communautaire"><label htmlFor="community-search">Rechercher un son</label><input id="community-search" type="search" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Titre ou description" />{session && <><label htmlFor="target-board">Soundboard d’import</label><select id="target-board" value={selectedBoard} onChange={(event) => setSelectedBoard(event.target.value)}>{library.data?.soundboards.map((board) => <option key={board.id} value={board.id}>{board.title}</option>)}</select></>}</section>
    <section aria-labelledby="discover-title"><div className="section-heading"><div><p className="eyebrow">Découvrir</p><h2 id="discover-title">Sons partagés</h2></div><span>{items.length} résultat{items.length > 1 ? "s" : ""}</span></div>
      {publications.isPending ? <p>Chargement de la communauté…</p> : items.length === 0 ? <p className="empty-state">Aucun son ne correspond à cette recherche.</p> : <div className="community-grid">{items.map((item) => {
        const active = previewId === item.id; const transfer = progress[item.id];
        return <article className="community-card" key={item.id}><div><p className="eyebrow">par {item.owner.username}</p><h3>{item.title}</h3><p>{item.description || "Aucune description"}</p></div><dl><div><dt>Durée</dt><dd>{item.audio.durationMs ? `${Math.round(item.audio.durationMs / 100) / 10} s` : "—"}</dd></div><div><dt>Taille</dt><dd>{formatBytes(item.audio.byteSize)}</dd></div></dl>{active && apiUrl.data && <audio ref={audioRef} src={`${apiUrl.data}/v1/publications/${item.id}/audio`} onEnded={() => setPreview(null)} preload="metadata" />}{transfer && <progress aria-label={`Progression de ${item.title}`} value={transfer.percent} max="100">{transfer.percent} %</progress>}<div className="actions"><button type="button" className="secondary-button" onClick={() => togglePreview(item.id)}>{active ? "Arrêter l’aperçu" : "Écouter"}</button>{session && <button type="button" className="primary-button" disabled={!selectedBoard || importSound.isPending} onClick={() => importSound.mutate({ publication: item, soundboardId: selectedBoard })}>Importer</button>}{ownedIds.has(item.id) && <button type="button" className="danger-button" onClick={() => remove.mutate(item.id)}>Retirer</button>}</div></article>;
      })}</div>}
      {publications.hasNextPage && <button className="secondary-button load-more" type="button" disabled={publications.isFetchingNextPage} onClick={() => void publications.fetchNextPage()}>{publications.isFetchingNextPage ? "Chargement…" : "Afficher plus"}</button>}
    </section>
    {session && <section aria-labelledby="publish-title" className="publish-section"><div className="section-heading"><div><p className="eyebrow">Mes sons</p><h2 id="publish-title">Publier explicitement</h2></div></div><div className="publish-list">{sounds.map((sound) => <article key={sound.id} className="publish-row"><div><strong>{sound.title}</strong><span>{sound.publicationId ? "Publié" : "Privé"}</span></div><label htmlFor={`description-${sound.id}`}>Description</label><input id={`description-${sound.id}`} maxLength={500} disabled={Boolean(sound.publicationId)} value={descriptions[sound.id] ?? ""} onChange={(event) => setDescriptions((current) => ({ ...current, [sound.id]: event.target.value }))} />{progress[sound.id] && <progress value={progress[sound.id]!.percent} max="100" />}{sound.publicationId ? <button type="button" className="danger-button" onClick={() => remove.mutate(sound.publicationId!)}>Retirer</button> : <button type="button" className="primary-button" disabled={publish.isPending} onClick={() => publish.mutate({ sound, description: descriptions[sound.id] ?? "" })}>{publish.isPending && publish.variables?.sound.id === sound.id ? "Publication…" : "Publier"}</button>}</article>)}</div></section>}
  </section>;
}
