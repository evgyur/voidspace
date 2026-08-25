fn main() {
    println!("cargo:rerun-if-changed=voidspace-elevated.manifest");
    println!("cargo:rerun-if-changed=voidspace-elevated.rc");
    embed_resource::compile("voidspace-elevated.rc", embed_resource::NONE)
        .manifest_required()
        .expect("compile the embedded requireAdministrator manifest");
}
