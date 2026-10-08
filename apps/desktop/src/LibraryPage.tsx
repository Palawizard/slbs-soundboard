import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { CSSProperties, FormEvent, KeyboardEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AudioStatus } from "./audioStatus";
import { ArrowLeftIcon, ArrowRightIcon, CloseIcon, ImageIcon, MoreIcon, PlusIcon, SearchIcon, StopIcon, TrashIcon, TuneIcon } from "./icons";
import { LibrarySnapshot, PlaybackProfile, Sound, Soundboard, loadLibrary, moveItem } from "./library";
import { shortcutFromKeyboardEvent, shortcutLabel, useGlobalShortcuts } from "./shortcuts";

function formatDuration(milliseconds: number) {
  const seconds = Math.max(0, Math.round(milliseconds / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function plural(count: number, word: string) {
  return `${count} ${word}${count > 1 ? "s" : ""}`;
}

/// A sound without a picture still gets a stable face: a slate socket tinted
/// within a narrow blue-green band from the audio hash, carrying its own
/// waveform rather than placeholder initials.
function fallbackStyle(seed: string): CSSProperties {
  let hash = 0;
  for (const character of seed) hash = (hash * 31 + character.charCodeAt(0)) | 0;
  return { "--art-hue": 195 + (Math.abs(hash) % 40) } as CSSProperties;
}

function WaveGlyph({ peaks }: { peaks: number[] }) {
  if (peaks.length === 0) return null;
  const step = Math.max(1, Math.floor(peaks.length / 24));
  const bars = peaks.filter((_, index) => index % step === 0).slice(0, 24);
  return (
    <svg className="slot-wave" viewBox={`0 0 ${bars.length * 4} 24`} preserveAspectRatio="none" aria-hidden="true">
      {bars.map((peak, index) => { const height = Math.max(2, Math.min(1, peak) * 22); return <rect key={index} x={index * 4 + 1} y={(24 - height) / 2} width="2" height={height} rx="1" />; })}
    </svg>
  );
}

function SoundArtwork({ sound }: { sound: Sound }) {
  const [source, setSource] = useState<string | null>(null);
  useEffect(() => {
    if (!sound.imageHash) { setSource(null); return; }
    let active = true;
    void invoke<string>("sound_image_data", { hash: sound.imageHash })
      .then((value) => { if (active) setSource(value); })
      .catch(() => { if (active) setSource(null); });
    return () => { active = false; };
  }, [sound.imageHash]);
  if (source) return <img className="slot-art" src={source} alt="" />;
  return <span className="slot-art slot-art-fallback" style={fallbackStyle(sound.audioHash)} aria-hidden="true"><WaveGlyph peaks={sound.waveform} /></span>;
}

type SlotProps = {
  sound: Sound;
  boardTitle?: string;
  playing: boolean;
  progress: number | null;
  selected: boolean;
  enterTarget: boolean;
  reducedMotion: boolean;
  onPlay: (sound: Sound) => void;
  onStop: (sound: Sound) => void;
  onSelect: (sound: Sound) => void;
};

function SoundSlot({ sound, boardTitle, playing, progress, selected, enterTarget, reducedMotion, onPlay, onStop, onSelect }: SlotProps) {
  // Under reduced motion the cooldown holds still: a steady veil, no per-poll steps.
  const sweep = { "--remaining": reducedMotion ? 1 : Math.max(0, 1 - (progress ?? 0)) } as CSSProperties;
  const keybind = sound.playback.keybind;
  return (
    <li className="slot" data-playing={playing || undefined} data-selected={selected || undefined} data-target={enterTarget || undefined}>
      <div className="slot-face">
        <button
          className="slot-trigger"
          type="button"
          aria-label={`Jouer ${sound.title}`}
          aria-describedby={keybind ? `key-${sound.id}` : undefined}
          onClick={() => onPlay(sound)}
          onContextMenu={(event) => { event.preventDefault(); onSelect(sound); }}
        >
          <SoundArtwork sound={sound} />
          {playing && (progress !== null || reducedMotion) && <span className={`slot-sweep ${reducedMotion ? "still" : ""}`} style={sweep} aria-hidden="true" />}
          {keybind && <kbd className="slot-key" id={`key-${sound.id}`}>{shortcutLabel(keybind)}</kbd>}
          {enterTarget && <kbd className="slot-enter" aria-hidden="true">Entrée</kbd>}
        </button>
        <div className="slot-tools">
          {playing && <button className="slot-tool slot-stop" type="button" onClick={() => onStop(sound)} aria-label={`Arrêter ${sound.title}`} title="Arrêter ce son"><StopIcon /></button>}
          <button className="slot-tool" type="button" onClick={() => onSelect(sound)} aria-label={`Régler ${sound.title}`} aria-pressed={selected} title="Régler ce son"><TuneIcon /></button>
        </div>
      </div>
      <div className="slot-meta">
        <span className="slot-title" title={sound.title}>{sound.title}</span>
        <span className="slot-time">{formatDuration(sound.durationMs)}</span>
      </div>
      {boardTitle && <span className="slot-board">{boardTitle}</span>}
    </li>
  );
}

const replayLabels = { overlap: "Superposer", toggle: "Pause ou reprendre", stop: "Arrêter", restart: "Recommencer" } as const;

type InspectorProps = {
  sound: Sound;
  index: number;
  total: number;
  playing: boolean;
  busy: boolean;
  onClose: () => void;
  onRename: (sound: Sound, title: string) => void;
  onImage: (sound: Sound) => void;
  onDelete: (sound: Sound) => void;
  onMove: (from: number, to: number) => void;
  onProfile: (sound: Sound, profile: PlaybackProfile, restart: boolean) => void;
  shortcutOwner: (shortcut: string, soundId: string) => string | null;
};

function SoundInspector({ sound, index, total, playing, busy, onClose, onRename, onImage, onDelete, onMove, onProfile, shortcutOwner }: InspectorProps) {
  const [title, setTitle] = useState(sound.title);
  const [draft, setDraft] = useState(sound.playback);
  const [capturing, setCapturing] = useState(false);
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const titleRef = useRef<HTMLInputElement>(null);
  useEffect(() => { setTitle(sound.title); }, [sound.id, sound.title]);
  useEffect(() => { setDraft(sound.playback); setCapturing(false); setShortcutError(null); }, [sound.id, sound.playback]);
  useEffect(() => { titleRef.current?.focus(); }, [sound.id]);
  useEffect(() => {
    if (draft === sound.playback) return;
    // A sound being listened to restarts with the new setting; a silent one waits.
    const timer = window.setTimeout(() => onProfile(sound, draft, playing), 400);
    return () => window.clearTimeout(timer);
  }, [draft, sound, onProfile, playing]);
  const updateDraft = (next: Partial<PlaybackProfile>) => setDraft((current) => ({ ...current, ...next }));

  const commitTitle = () => {
    const next = title.trim();
    if (next && next !== sound.title) onRename(sound, next); else setTitle(sound.title);
  };
  const captureShortcut = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!capturing) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.code === "Escape") { setCapturing(false); setShortcutError(null); return; }
    if (event.code === "Backspace" || event.code === "Delete") { updateDraft({ keybind: null }); setCapturing(false); setShortcutError(null); return; }
    const shortcut = shortcutFromKeyboardEvent(event.nativeEvent);
    if (!shortcut) { setShortcutError("Utilisez au moins Ctrl, Alt, Maj ou Windows avec une touche prise en charge."); return; }
    const owner = shortcutOwner(shortcut, sound.id);
    if (owner) { setShortcutError(`Ce raccourci est déjà utilisé par « ${owner} ».`); return; }
    updateDraft({ keybind: shortcut });
    setCapturing(false);
    setShortcutError(null);
  };

  return (
    <section className="inspector" aria-labelledby="inspector-title" onKeyDown={(event) => { if (event.key === "Escape" && !capturing) onClose(); }}>
      <header className="inspector-head">
        <h2 id="inspector-title">Réglages du son</h2>
        <button className="icon-button" type="button" onClick={onClose} aria-label="Fermer les réglages"><CloseIcon /></button>
      </header>
      <form className="field" onSubmit={(event) => { event.preventDefault(); commitTitle(); }}>
        <label htmlFor="sound-title">Nom</label>
        <input id="sound-title" ref={titleRef} value={title} maxLength={120} onChange={(event) => setTitle(event.target.value)} onBlur={commitTitle} disabled={busy} />
      </form>

      <div className="field">
        <span className="field-label" id="shortcut-label">Raccourci global</span>
        <div className="shortcut-row">
          <button type="button" className={`keycap-field ${capturing ? "capturing" : ""}`} aria-labelledby="shortcut-label shortcut-value" onClick={() => { setCapturing(true); setShortcutError(null); }} onKeyDown={captureShortcut} onBlur={() => setCapturing(false)}>
            <kbd id="shortcut-value">{capturing ? "Appuyez sur les touches…" : shortcutLabel(draft.keybind)}</kbd>
          </button>
          {draft.keybind && !capturing && <button type="button" className="text-button" onClick={() => updateDraft({ keybind: null })}>Effacer</button>}
        </div>
        {shortcutError
          ? <p className="field-error" role="alert">{shortcutError}</p>
          : <p className="field-hint">{capturing ? "Échap pour annuler, Retour arrière pour retirer." : "Cliquez puis appuyez sur la combinaison voulue."}</p>}
      </div>

      <div className="field-group">
        <label className="range-field">
          <span>Volume</span><output>{Math.round(draft.volume * 100)} %</output>
          <input type="range" min="0" max="2" step="0.05" value={draft.volume} onChange={(event) => updateDraft({ volume: Number(event.target.value) })} />
        </label>
        <label className="range-field">
          <span>Hauteur</span><output>{draft.pitchSemitones > 0 ? "+" : ""}{draft.pitchSemitones} demi-ton{Math.abs(draft.pitchSemitones) > 1 ? "s" : ""}</output>
          <input type="range" min="-12" max="12" step="1" value={draft.pitchSemitones} onChange={(event) => updateDraft({ pitchSemitones: Number(event.target.value) })} />
        </label>
        <label className="range-field">
          <span>Vitesse</span><output>{draft.speed.toFixed(2)}×</output>
          <input type="range" min="0.5" max="2" step="0.05" value={draft.speed} onChange={(event) => updateDraft({ speed: Number(event.target.value) })} />
        </label>
        <div className="field">
          <label htmlFor="replay-policy">Au second appui</label>
          <select id="replay-policy" value={draft.replayPolicy} onChange={(event) => updateDraft({ replayPolicy: event.target.value as PlaybackProfile["replayPolicy"] })}>
            {Object.entries(replayLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </select>
        </div>
        <p className="field-hint" role="status">Enregistré automatiquement. Un son en cours repart aussitôt avec le nouveau réglage.</p>
      </div>

      <div className="inspector-actions">
        <button type="button" className="ghost-button" onClick={() => onImage(sound)} disabled={busy}><ImageIcon />Choisir une image</button>
        <div className="move-pair" role="group" aria-label="Position dans le soundboard">
          <button type="button" className="icon-button" onClick={() => onMove(index, index - 1)} disabled={busy || index === 0} aria-label="Déplacer avant"><ArrowLeftIcon /></button>
          <span>{index + 1} / {total}</span>
          <button type="button" className="icon-button" onClick={() => onMove(index, index + 1)} disabled={busy || index === total - 1} aria-label="Déplacer après"><ArrowRightIcon /></button>
        </div>
        <button type="button" className="ghost-button danger" onClick={() => onDelete(sound)} disabled={busy}><TrashIcon />Supprimer le son</button>
      </div>
    </section>
  );
}

function BoardMenu({ board, index, total, busy, onRename, onDelete, onMove }: { board: Soundboard; index: number; total: number; busy: boolean; onRename: () => void; onDelete: () => void; onMove: (from: number, to: number) => void }) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => { if (!rootRef.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);
  const act = (action: () => void) => { setOpen(false); action(); };
  return (
    <div className="board-menu" ref={rootRef} onKeyDown={(event) => { if (event.key === "Escape") setOpen(false); }}>
      <button type="button" className="icon-button" aria-expanded={open} aria-controls="board-menu-list" aria-label={`Options de « ${board.title} »`} onClick={() => setOpen((value) => !value)} disabled={busy}><MoreIcon /></button>
      {open && (
        <div className="menu" id="board-menu-list">
          <button type="button" onClick={() => act(onRename)}>Renommer</button>
          <button type="button" onClick={() => act(() => onMove(index, index - 1))} disabled={index === 0}>Déplacer à gauche</button>
          <button type="button" onClick={() => act(() => onMove(index, index + 1))} disabled={index === total - 1}>Déplacer à droite</button>
          <button type="button" className="danger" onClick={() => act(onDelete)}>Supprimer le soundboard</button>
        </div>
      )}
    </div>
  );
}

function useReducedMotion() {
  const query = "(prefers-reduced-motion: reduce)";
  const [reduced, setReduced] = useState(() => typeof window.matchMedia === "function" && window.matchMedia(query).matches);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const list = window.matchMedia(query);
    const update = () => setReduced(list.matches);
    list.addEventListener("change", update);
    return () => list.removeEventListener("change", update);
  }, []);
  return reduced;
}

export function LibraryPage({ audio, active }: { audio: AudioStatus; active: boolean }) {
  const [snapshot, setSnapshot] = useState<LibrarySnapshot>({ soundboards: [], recoveryNotice: null, activeSoundboardId: null });
  const [selectedId, setSelectedId] = useState("");
  const [inspectedId, setInspectedId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [creating, setCreating] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [playing, setPlaying] = useState<{ id: string; totalFrames: number; startedAt: number } | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const reducedMotion = useReducedMotion();
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const reportShortcutError = useCallback((message: string) => setError(message), []);
  useGlobalShortcuts(snapshot.soundboards, reportShortcutError);

  const refresh = useCallback(async (preferredId?: string) => {
    const next = await loadLibrary();
    setSnapshot(next);
    setNotice(next.recoveryNotice);
    setSelectedId((current) => {
      const preferred = preferredId ?? (current || next.activeSoundboardId || "");
      return next.soundboards.some((board) => board.id === preferred) ? preferred : (next.soundboards[0]?.id ?? "");
    });
  }, []);
  useEffect(() => { void refresh().catch((reason: unknown) => setError(String(reason))); }, [refresh]);

  // "/" or Ctrl+F jumps to the search, from anywhere outside a text field.
  useEffect(() => {
    if (!active) return;
    const focusSearch = (event: globalThis.KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target?.closest("input, textarea, select, [contenteditable='true']");
      if ((event.key === "/" && !typing) || (event.ctrlKey && event.key.toLowerCase() === "f")) {
        event.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, [active]);

  // The engine names the sounds it plays, so keybinds and clicks share one truth.
  const activeIds = useMemo(() => new Set(audio.activeSoundIds ?? []), [audio.activeSoundIds]);
  // Give the engine one poll to report a sound before treating it as finished.
  useEffect(() => { if (playing && !activeIds.has(playing.id) && Date.now() - playing.startedAt > 600) setPlaying(null); }, [activeIds, playing]);
  const progress = playing && audio.playbackTotalFrames === playing.totalFrames
    ? Math.min(1, audio.playbackFrames / Math.max(1, playing.totalFrames)) : null;
  const selected = useMemo(() => snapshot.soundboards.find((board) => board.id === selectedId) ?? snapshot.soundboards[0], [snapshot.soundboards, selectedId]);
  const allSounds = useMemo(() => snapshot.soundboards.flatMap((board) => board.sounds.map((sound) => ({ sound, board }))), [snapshot.soundboards]);
  const trimmedQuery = query.trim().toLocaleLowerCase("fr");
  const results = useMemo(() => trimmedQuery
    ? allSounds.filter(({ sound }) => sound.title.toLocaleLowerCase("fr").includes(trimmedQuery))
    : (selected?.sounds ?? []).map((sound) => ({ sound, board: selected! })), [allSounds, selected, trimmedQuery]);
  const inspected = inspectedId ? allSounds.find(({ sound }) => sound.id === inspectedId) : undefined;
  useEffect(() => { if (inspectedId && !inspected) setInspectedId(null); }, [inspected, inspectedId]);

  async function run(action: () => Promise<unknown>, success?: string, preferredId?: string) {
    setBusy(true); setError(null); setNotice(null);
    try { await action(); await refresh(preferredId); if (success) setNotice(success); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  function selectBoard(board: Soundboard) {
    setSelectedId(board.id);
    setQuery("");
    void invoke("select_soundboard", { id: board.id }).catch((reason: unknown) => setError(String(reason)));
  }

  function onTabKey(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    const count = snapshot.soundboards.length;
    const target = event.key === "ArrowRight" ? (index + 1) % count : event.key === "ArrowLeft" ? (index - 1 + count) % count
      : event.key === "Home" ? 0 : event.key === "End" ? count - 1 : -1;
    if (target < 0) return;
    event.preventDefault();
    selectBoard(snapshot.soundboards[target]);
    tabRefs.current[target]?.focus();
  }

  async function createBoard(event: FormEvent) {
    event.preventDefault();
    const title = newTitle.trim(); if (!title) return;
    setNewTitle(""); setCreating(false);
    setBusy(true); setError(null); setNotice(null);
    try {
      const board = await invoke<Soundboard>("create_soundboard", { title });
      await invoke("select_soundboard", { id: board.id });
      await refresh(board.id);
      setNotice("Soundboard créé.");
    } catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }

  async function renameBoard(board: Soundboard) {
    const title = window.prompt("Nouveau nom", board.title)?.trim();
    if (title && title !== board.title) await run(() => invoke("rename_soundboard", { id: board.id, title }), "Soundboard renommé.", board.id);
  }

  async function deleteBoard(board: Soundboard) {
    if (!window.confirm(`Supprimer « ${board.title} » ? Les sons uniquement présents ici seront aussi supprimés.`)) return;
    await run(() => invoke("delete_soundboard", { id: board.id }), "Soundboard supprimé.");
  }

  async function moveBoard(from: number, to: number) {
    const reordered = moveItem(snapshot.soundboards, from, to); if (reordered === snapshot.soundboards) return;
    setSnapshot((current) => ({ ...current, soundboards: reordered }));
    await run(() => invoke("reorder_soundboards", { ids: reordered.map((board) => board.id) }), undefined, selectedId);
  }

  async function importSounds() {
    if (!selected) return;
    const paths = await open({ multiple: true, filters: [{ name: "Sons", extensions: ["wav", "mp3", "flac", "ogg", "m4a", "aac"] }] });
    if (!paths) return;
    const files = Array.isArray(paths) ? paths : [paths];
    await run(async () => { for (const path of files) await invoke("import_sound", { soundboardId: selected.id, path }); }, files.length > 1 ? `${files.length} sons ajoutés.` : "Son ajouté.", selected.id);
  }

  function renameSound(sound: Sound, title: string) {
    void run(() => invoke("rename_sound", { soundId: sound.id, title }), "Son renommé.", selected?.id);
  }

  async function chooseImage(sound: Sound) {
    const path = await open({ multiple: false, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"] }] });
    if (typeof path === "string") await run(() => invoke("set_sound_image", { soundId: sound.id, path }), "Image mise à jour.", selected?.id);
  }

  async function deleteSound(sound: Sound, board: Soundboard) {
    if (!window.confirm(`Supprimer « ${sound.title} » de « ${board.title} » ?`)) return;
    setInspectedId(null);
    await run(() => invoke("delete_sound", { soundboardId: board.id, soundId: sound.id }), "Son supprimé.", selected?.id);
  }

  async function moveSound(board: Soundboard, from: number, to: number) {
    const sounds = moveItem(board.sounds, from, to); if (sounds === board.sounds) return;
    setSnapshot((current) => ({ ...current, soundboards: current.soundboards.map((item) => item.id === board.id ? { ...item, sounds } : item) }));
    await run(() => invoke("reorder_sounds", { soundboardId: board.id, ids: sounds.map((sound) => sound.id) }), undefined, selected?.id);
  }

  async function playSound(sound: Sound) {
    setError(null);
    try {
      const totalFrames = await invoke<number>("play_sound", { soundId: sound.id });
      setPlaying({ id: sound.id, totalFrames, startedAt: Date.now() });
    } catch (reason) {
      setError(String(reason));
    }
  }

  const updateProfile = useCallback(async (sound: Sound, profile: PlaybackProfile, restart: boolean) => {
    setError(null);
    try {
      await invoke("update_playback_profile", { soundId: sound.id, profile });
      setSnapshot((current) => ({ ...current, soundboards: current.soundboards.map((board) => ({ ...board, sounds: board.sounds.map((item) => item.id === sound.id ? { ...item, playback: profile } : item) })) }));
      if (restart) {
        await invoke("stop_sound", { soundId: sound.id });
        const totalFrames = await invoke<number>("play_sound", { soundId: sound.id });
        setPlaying({ id: sound.id, totalFrames, startedAt: Date.now() });
      }
    } catch (reason) { setError(String(reason)); }
  }, []);

  const shortcutOwner = useCallback((shortcut: string, soundId: string) => {
    const owner = snapshot.soundboards.flatMap((board) => board.sounds)
      .find((sound) => sound.id !== soundId && sound.playback.keybind?.toLowerCase() === shortcut.toLowerCase());
    return owner?.title ?? null;
  }, [snapshot.soundboards]);

  async function stopSound(sound: Sound) {
    try { await invoke("stop_sound", { soundId: sound.id }); setPlaying((current) => current?.id === sound.id ? null : current); }
    catch (reason) { setError(String(reason)); }
  }

  const toggleInspector = useCallback((sound: Sound) => setInspectedId((current) => current === sound.id ? null : sound.id), []);
  const boardIndex = selected ? snapshot.soundboards.findIndex((board) => board.id === selected.id) : -1;
  const hasBoards = snapshot.soundboards.length > 0;

  return (
    <div className={`library ${inspected ? "with-inspector" : ""}`}>
      <h1 className="sr-only">Sons</h1>
      <header className="toolbar">
        <div className="board-tabs">
          {hasBoards && (
            <div className="tablist" role="tablist" aria-label="Soundboards">
              {snapshot.soundboards.map((board, index) => {
                const active = !trimmedQuery && selected?.id === board.id;
                return (
                  <button
                    key={board.id}
                    ref={(element) => { tabRefs.current[index] = element; }}
                    className="tab"
                    type="button"
                    role="tab"
                    aria-selected={active}
                    tabIndex={selected?.id === board.id ? 0 : -1}
                    onClick={() => selectBoard(board)}
                    onKeyDown={(event) => onTabKey(event, index)}
                  >
                    <span>{board.title}</span><span className="tab-count">{board.sounds.length}</span>
                  </button>
                );
              })}
            </div>
          )}
          {selected && <BoardMenu board={selected} index={boardIndex} total={snapshot.soundboards.length} busy={busy} onRename={() => void renameBoard(selected)} onDelete={() => void deleteBoard(selected)} onMove={(from, to) => void moveBoard(from, to)} />}
          {creating ? (
            <form className="new-board" onSubmit={createBoard}>
              <label className="sr-only" htmlFor="new-board-title">Nom du soundboard</label>
              <input id="new-board-title" autoFocus value={newTitle} maxLength={80} onChange={(event) => setNewTitle(event.target.value)} onKeyDown={(event) => { if (event.key === "Escape") { setCreating(false); setNewTitle(""); } }} placeholder="Nouveau soundboard" disabled={busy} />
              <button className="icon-button" type="submit" disabled={busy || !newTitle.trim()} aria-label="Créer le soundboard"><PlusIcon /></button>
            </form>
          ) : (
            <button className="icon-button" type="button" onClick={() => setCreating(true)} aria-label="Nouveau soundboard" title="Nouveau soundboard"><PlusIcon /></button>
          )}
        </div>
        <div className="toolbar-actions">
        <div className="search">
          <SearchIcon />
          <label className="sr-only" htmlFor="sound-search">Rechercher un son</label>
          <input
            id="sound-search"
            ref={searchRef}
            type="search"
            value={query}
            placeholder="Rechercher un son"
            autoComplete="off"
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && results[0]) { event.preventDefault(); void playSound(results[0].sound); }
              if (event.key === "Escape") setQuery("");
            }}
          />
          <kbd className="search-hint" aria-hidden="true">/</kbd>
        </div>
        <button className="primary-button" type="button" onClick={importSounds} disabled={!selected || busy}><PlusIcon />{busy ? "Patientez…" : "Ajouter des sons"}</button>
        </div>
      </header>

      {(notice || error) && (
        <div className="messages">
          {notice && <p className="message" role="status">{notice}</p>}
          {error && <p className="message message-error" role="alert">{error}</p>}
        </div>
      )}

      <section className="grid-area" aria-label={trimmedQuery ? "Résultats de la recherche" : selected ? `Sons de ${selected.title}` : "Sons"}>
        {trimmedQuery && <p className="result-line" role="status">{results.length === 0 ? `Aucun son ne correspond à « ${query.trim()} ».` : `${plural(results.length, "son")} dans tous vos soundboards · Entrée joue le premier`}</p>}
        {results.length > 0 ? (
          <ul className="slot-grid">
            {results.map(({ sound, board }, index) => (
              <SoundSlot
                key={sound.id}
                sound={sound}
                boardTitle={trimmedQuery ? board.title : undefined}
                playing={activeIds.has(sound.id)}
                progress={playing?.id === sound.id ? progress : null}
                selected={inspectedId === sound.id}
                enterTarget={Boolean(trimmedQuery) && index === 0}
                reducedMotion={reducedMotion}
                onPlay={playSound}
                onStop={stopSound}
                onSelect={toggleInspector}
              />
            ))}
          </ul>
        ) : !trimmedQuery && (
          <div className="empty">
            {hasBoards ? (
              <>
                <h2>Ce soundboard est vide</h2>
                <p>Ajoutez des fichiers WAV, MP3, FLAC, OGG, M4A ou AAC. Chaque son pourra recevoir un raccourci global.</p>
                <button className="primary-button" type="button" onClick={importSounds} disabled={!selected || busy}><PlusIcon />Choisir des sons</button>
              </>
            ) : (
              <>
                <h2>Créez votre premier soundboard</h2>
                <p>Un soundboard regroupe des sons, par jeu ou par groupe d’amis.</p>
                <button className="primary-button" type="button" onClick={() => setCreating(true)}><PlusIcon />Nouveau soundboard</button>
              </>
            )}
          </div>
        )}
      </section>

      {inspected && (
        <SoundInspector
          sound={inspected.sound}
          index={inspected.board.sounds.findIndex((item) => item.id === inspected.sound.id)}
          total={inspected.board.sounds.length}
          playing={activeIds.has(inspected.sound.id)}
          busy={busy}
          onClose={() => setInspectedId(null)}
          onRename={renameSound}
          onImage={chooseImage}
          onDelete={(sound) => void deleteSound(sound, inspected.board)}
          onMove={(from, to) => void moveSound(inspected.board, from, to)}
          onProfile={updateProfile}
          shortcutOwner={shortcutOwner}
        />
      )}
    </div>
  );
}
