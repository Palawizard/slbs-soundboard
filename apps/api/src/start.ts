import { AuthService } from "./auth/service.js";
import { ProductionGoogleIdentityProvider } from "./auth/google.js";
import { loadConfig } from "./config.js";
import { migrateDatabase } from "./database/migrate.js";
import { PostgresCommunityRepository } from "./database/postgres.js";
import { ProductionMediaInspector } from "./media/inspector.js";
import { MediaService } from "./media/service.js";
import { LocalObjectStorage } from "./media/storage.js";
import { PublicationService } from "./publications/service.js";
import { buildServer } from "./server.js";

const config = loadConfig();
const repository = new PostgresCommunityRepository(config.databaseUrl);
await migrateDatabase(repository.pool);
const storage = new LocalObjectStorage(config.mediaRoot);
await storage.initialize();
const google = new ProductionGoogleIdentityProvider(config.googleClientId, config.googleClientSecret);
const auth = new AuthService(repository, google, config.sessionSecret);
const media = new MediaService(repository, storage, new ProductionMediaInspector(config.ffprobePath));
const publications = new PublicationService(repository, config.publicBaseUrl, config.sessionSecret);
const server = buildServer({ config, repository, auth, media, publications, storage });
server.addHook("onClose", async () => repository.close());

try { await server.listen({ host: config.host, port: config.port }); }
catch (error) { server.log.error(error); await repository.close(); process.exitCode = 1; }
