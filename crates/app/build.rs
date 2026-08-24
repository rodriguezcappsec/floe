fn main() {
    glib_build_tools::compile_resources(
        &["resources"],
        "resources/floe.gresource.xml",
        "floe.gresource",
    );
}
