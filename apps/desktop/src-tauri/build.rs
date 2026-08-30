fn main() {
    for variable in [
        "ARCMETER_SUPABASE_URL",
        "ARCMETER_SUPABASE_ANON_KEY",
        "VITE_SUPABASE_URL",
        "VITE_SUPABASE_ANON_KEY",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }

    tauri_build::build()
}
