import "./App.css";

function App() {
  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand-mark">SLB</div>
        <nav aria-label="Navigation principale">
          <button className="nav-item active" type="button">Mes soundboards</button>
          <button className="nav-item" type="button">Communauté</button>
          <button className="nav-item" type="button">Paramètres</button>
        </nav>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">SLB's Soundboard</p>
            <h1>Vos sons, au bon moment.</h1>
          </div>
          <span className="status">Socle prêt</span>
        </header>

        <section className="empty-state" aria-labelledby="welcome-title">
          <div className="pulse" aria-hidden="true" />
          <p className="eyebrow">Nouveau projet</p>
          <h2 id="welcome-title">Le studio prend forme</h2>
          <p>
            Le prochain jalon connectera votre microphone au moteur audio et au
            microphone virtuel.
          </p>
        </section>
      </section>
    </main>
  );
}

export default App;
