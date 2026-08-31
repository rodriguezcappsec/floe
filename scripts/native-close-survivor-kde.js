const floeWindows = workspace.windowList().filter((window) => {
    const identity = [
        window.resourceClass,
        window.resourceName,
        window.desktopFileName,
        window.caption,
    ].map((value) => String(value).toLowerCase()).join(" ");
    return identity.includes("floe");
});

for (const candidate of floeWindows) {
    const sameProcess = floeWindows.filter((window) => window.pid === candidate.pid);
    if (sameProcess.length === 2) {
        sameProcess[1].closeWindow();
        break;
    }
}
