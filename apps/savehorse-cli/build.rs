//! Windows gives an executable its icon and its version strings from a
//! resource block compiled into the binary. Without one, savehorse.exe shows
//! the generic application icon in Explorer and in the task bar, which is a
//! strange thing to ship.
//!
//! This only runs when building on Windows; everywhere else it does nothing.

fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../apps/savehorse-desktop/src-tauri/icons/icon.ico");
        res.set("ProductName", "savehorse");
        res.set("FileDescription", "savehorse — one bench, thirteen tools");
        res.set("LegalCopyright", "© 2026 Dr Sohil Momin. PolyForm Noncommercial 1.0.0.");
        res.set("OriginalFilename", "savehorse.exe");
        if let Err(error) = res.compile() {
            // A missing resource compiler should not stop the build: an
            // executable without an icon still works.
            println!("cargo:warning=could not embed the Windows icon: {}", error);
        }
    }
}
