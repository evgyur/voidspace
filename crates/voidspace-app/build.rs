fn main() {
    println!("cargo:rerun-if-changed=voidspace-app.manifest");
    println!("cargo:rerun-if-changed=voidspace-app.rc");
    embed_resource::compile_for("voidspace-app.rc", ["voidspace"], embed_resource::NONE)
        .manifest_required()
        .expect("compile the embedded requireAdministrator manifest");
}
