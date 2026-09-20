/// Build-time feature switches, read from Vite environment variables.
///
/// The community service and the kernel-mode driver are both complete but are
/// not part of the current usable build: the community needs a deployed server
/// and a Google client, and the driver needs a production Microsoft signature.
/// Setting the matching variable to "true" at build time brings them back.
export const communityEnabled = import.meta.env.VITE_SLB_COMMUNITY === "true";
export const driverEnabled = import.meta.env.VITE_SLB_DRIVER === "true";
