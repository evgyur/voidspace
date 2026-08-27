fn main() {
    println!("cargo:rerun-if-changed=voidspace-app.manifest");
    println!("cargo:rerun-if-changed=voidspace-app.rc");
    println!("cargo:rerun-if-changed=assets/voidspace.ico");
    embed_resource::compile_for("voidspace-app.rc", ["voidspace"], embed_resource::NONE)
        .manifest_required()
        .expect("compile the embedded icon and requireAdministrator manifest");
}
