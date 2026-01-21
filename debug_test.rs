use auto_lang::config::AutoConfig;
use auto_val::Obj;

fn main() {
    let config_path = "tests/integration/001_simple_app/pac.at";
    let config = AutoConfig::read(config_path).unwrap();
    
    let port_nodes = config.root.get_nodes("port");
    for port_node in port_nodes {
        println!("Port name: {}", port_node.main_arg().to_astr());
        println!("Port builder: {}", port_node.get_prop("builder").to_astr());
        println!("Port platform: {}", port_node.get_prop("platform").to_astr());
        
        // Check props
        if port_node.has_prop("builder") {
            let builder_prop = port_node.get_prop("builder");
            println!("Builder prop type: {:?}", builder_prop.get_type());
            println!("Builder prop as string: {}", builder_prop.to_astr());
        }
    }
}
