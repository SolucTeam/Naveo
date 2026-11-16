//! Affiche les contextes Kubernetes découverts (vérification manuelle).
//! Usage : cargo run --example list-contexts -p naveo-core

fn main() {
    let contexts = naveo_core::k8s::list_contexts();
    println!("{} contexte(s) trouvé(s) :\n", contexts.len());
    for c in &contexts {
        println!("  {:45} cluster: {}", c.name, c.cluster);
    }
}
