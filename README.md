Vulkano-shaders is a monolithic system for compilation (shaderc), build-time reflection (shader interfaces), and linking
In this simpler modular alternative system, this crate only focus on build-time reflection (shader interfaces).
Suggestions :
- Compile WGSL files using naga from your app/build.rs (easier/faster Rust build, WGSL not only GLSL, no shaderc (C/sys)) and static link SPIRV binaries
- Use helpers from ui/vulkan.rs to use linked SPIRV modules
- Use (this crate) build-time reflection on the SPIR-V files to get uniforms and vertex attributes types (ui/vulkan.rs shader! does this)
