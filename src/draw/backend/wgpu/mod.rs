use crate::draw;
use crate::frame::Frame;
use crate::math::{BaseFloat, NumCast};
use crate::wgpu;

/// A helper type aimed at simplifying the rendering of conrod primitives via wgpu.
#[derive(Debug)]
pub struct Renderer {
    _vs_mod: wgpu::ShaderModule,
    _fs_mod: wgpu::ShaderModule,
    render_pipeline: wgpu::RenderPipeline,
    depth_texture: wgpu::Texture,
    depth_texture_view: wgpu::TextureView,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

#[derive(Debug)]
pub struct DrawError;

/// The `Vertex` type passed to the vertex shader.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, PartialOrd)]
pub struct Vertex {
    /// The position of the vertex within vector space.
    ///
    /// [-1.0, 1.0, 0.0] is the leftmost, bottom position of the display.
    /// [1.0, -1.0, 0.0] is the rightmost, top position of the display.
    pub position: [f32; 3],
    /// A color associated with the `Vertex`.
    ///
    /// These values should be in the linear sRGB format.
    ///
    /// The way that the color is used depends on the `mode`.
    pub color: [f32; 4],
    /// The coordinates of the texture used by this `Vertex`.
    ///
    /// [0.0, 0.0] is the leftmost, bottom position of the texture.
    /// [1.0, 1.0] is the rightmost, top position of the texture.
    pub tex_coords: [f32; 2],
    // /// The mode with which the `Vertex` will be drawn within the fragment shader.
    // ///
    // /// `0` for rendering text.
    // /// `1` for rendering an image.
    // /// `2` for rendering non-textured 2D geometry.
    // ///
    // /// If any other value is given, the fragment shader will not output any color.
    // pub mode: u32,
}

impl wgpu::VertexDescriptor for Vertex {
    const STRIDE: wgpu::BufferAddress = std::mem::size_of::<Self>() as _;
    const ATTRIBUTES: &'static [wgpu::VertexAttributeDescriptor] = {
        let position_offset = 0;
        let position_size = std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress;
        let rgba_offset = position_offset + position_size;
        let rgba_size = std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress;
        let tex_coords_offset = rgba_offset + rgba_size;
        &[
            // position
            wgpu::VertexAttributeDescriptor {
                format: wgpu::VertexFormat::Float3,
                offset: position_offset,
                shader_location: 0,
            },
            // rgba
            wgpu::VertexAttributeDescriptor {
                format: wgpu::VertexFormat::Float4,
                offset: rgba_offset,
                shader_location: 1,
            },
            // tex_coords
            wgpu::VertexAttributeDescriptor {
                format: wgpu::VertexFormat::Float2,
                offset: tex_coords_offset,
                shader_location: 2,
            },
        ]
    };
}

impl Vertex {
    /// Create a vertex from the given mesh vertex.
    pub fn from_mesh_vertex<S>(
        v: draw::mesh::Vertex<S>,
        framebuffer_width: f32,
        framebuffer_height: f32,
        dpi_factor: f32,
    ) -> Self
    where
        S: BaseFloat,
    {
        let point = v.point();
        let x_f: f32 = NumCast::from(point.x).unwrap();
        let y_f: f32 = NumCast::from(point.y).unwrap();
        let z_f: f32 = NumCast::from(point.z).unwrap();
        // Map coords from (-fb_dim, +fb_dim) to (-1.0, 1.0)
        // In wgpu, *y* increases in the downwards direction, so we negate it.
        let x = 2.0 * x_f * dpi_factor / framebuffer_width;
        let y = -(2.0 * y_f * dpi_factor / framebuffer_height);
        let z = 2.0 * z_f * dpi_factor / framebuffer_height;
        let tex_x = NumCast::from(v.tex_coords.x).unwrap();
        let tex_y = NumCast::from(v.tex_coords.y).unwrap();
        let position = [x, y, z];
        let (r, g, b, a) = v.color.into();
        let color = [r, g, b, a];
        let tex_coords = [tex_x, tex_y];
        Vertex {
            position,
            color,
            tex_coords,
        }
    }
}

impl Renderer {
    /// The default depth format
    pub const DEFAULT_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    /// Create a **Renderer** targeting an output attachment texture of the given description.
    pub fn from_texture_descriptor(
        device: &wgpu::Device,
        descriptor: &wgpu::TextureDescriptor,
    ) -> Self {
        Self::new(
            device,
            [descriptor.size.width, descriptor.size.height],
            descriptor.sample_count,
            descriptor.format,
        )
    }

    /// Construct a new `Renderer`.
    pub fn new(
        device: &wgpu::Device,
        output_attachment_size: [u32; 2],
        msaa_samples: u32,
        output_attachment_color_format: wgpu::TextureFormat,
    ) -> Self {
        Self::with_depth_format(
            device,
            output_attachment_size,
            msaa_samples,
            output_attachment_color_format,
            Self::DEFAULT_DEPTH_FORMAT,
        )
    }

    /// The same as **new**, but allows for manually specifying the depth format.
    pub fn with_depth_format(
        device: &wgpu::Device,
        output_attachment_size: [u32; 2],
        msaa_samples: u32,
        output_attachment_color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        // Load shader modules.
        let vs = include_bytes!("shaders/vert.spv");
        let vs_spirv = wgpu::read_spirv(std::io::Cursor::new(&vs[..]))
            .expect("failed to read hard-coded SPIRV");
        let vs_mod = device.create_shader_module(&vs_spirv);
        let fs = include_bytes!("shaders/frag.spv");
        let fs_spirv = wgpu::read_spirv(std::io::Cursor::new(&fs[..]))
            .expect("failed to read hard-coded SPIRV");
        let fs_mod = device.create_shader_module(&fs_spirv);

        // Create the depth texture.
        let depth_texture =
            create_depth_texture(device, output_attachment_size, depth_format, msaa_samples);
        let depth_texture_view = depth_texture.create_default_view();

        // Create the render pipeline.
        let bind_group_layout = bind_group_layout(device);
        let bind_group = bind_group(device, &bind_group_layout);
        let render_pipeline = render_pipeline(
            device,
            &bind_group_layout,
            &vs_mod,
            &fs_mod,
            output_attachment_color_format,
            depth_format,
            msaa_samples,
        );
        let vertices = vec![];
        let indices = vec![];

        Self {
            _vs_mod: vs_mod,
            _fs_mod: fs_mod,
            render_pipeline,
            depth_texture,
            depth_texture_view,
            bind_group_layout,
            bind_group,
            vertices,
            indices,
        }
    }

    /// Encode a render pass with the given **Draw**ing to the given `output_attachment`.
    ///
    /// If the **Draw**ing has been scaled for handling DPI, specify the necessary `scale_factor`
    /// for scaling back to the `output_attachment_size` (physical dimensions).
    ///
    /// If the `output_attachment` is multisampled and should be resolved to another texture,
    /// include the `resolve_target`.
    pub fn encode_render_pass<S>(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        draw: &draw::Draw<S>,
        scale_factor: f32,
        output_attachment_size: [u32; 2],
        output_attachment: &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
    ) where
        S: BaseFloat,
    {
        let Renderer {
            ref render_pipeline,
            ref mut vertices,
            ref mut indices,
            ref mut depth_texture,
            ref mut depth_texture_view,
            ref bind_group,
            ..
        } = *self;

        // Resize the depth texture if the output attachment size has changed.
        let depth_size = depth_texture.size();
        if output_attachment_size != depth_size {
            let depth_format = depth_texture.format();
            let sample_count = depth_texture.sample_count();
            *depth_texture =
                create_depth_texture(device, output_attachment_size, depth_format, sample_count);
            *depth_texture_view = depth_texture.create_default_view();
        }

        // Retrieve the clear values based on the bg color.
        let bg_color = draw.state.borrow().background_color;
        let (load_op, clear_color) = match bg_color {
            None => (wgpu::LoadOp::Load, wgpu::Color::TRANSPARENT),
            Some(color) => {
                let (r, g, b, a) = color.into();
                let (r, g, b, a) = (r as f64, g as f64, b as f64, a as f64);
                let clear_color = wgpu::Color { r, g, b, a };
                (wgpu::LoadOp::Clear, clear_color)
            }
        };

        // Create the vertex and index buffers.
        let [img_w, img_h] = output_attachment_size;
        let map_vertex = |v| Vertex::from_mesh_vertex(v, img_w as _, img_h as _, scale_factor);
        vertices.clear();
        vertices.extend(draw.raw_vertices().map(map_vertex));
        let vertex_buffer = device
            .create_buffer_mapped(vertices.len(), wgpu::BufferUsage::VERTEX)
            .fill_from_slice(&vertices[..]);
        indices.clear();
        indices.extend(draw.inner_mesh().indices().iter().map(|&u| u as u32));
        let index_buffer = device
            .create_buffer_mapped(indices.len(), wgpu::BufferUsage::INDEX)
            .fill_from_slice(&indices[..]);

        // Encode the render pass.
        let mut render_pass = wgpu::RenderPassBuilder::new()
            .color_attachment(output_attachment, |color| {
                color
                    .resolve_target(resolve_target)
                    .load_op(load_op)
                    .clear_color(clear_color)
            })
            .depth_stencil_attachment(&*depth_texture_view, |depth| depth)
            .begin(encoder);
        render_pass.set_pipeline(render_pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_index_buffer(&index_buffer, 0);
        render_pass.set_vertex_buffers(0, &[(&vertex_buffer, 0)]);
        let index_range = 0..indices.len() as u32;
        let start_vertex = 0;
        let instance_range = 0..1;
        render_pass.draw_indexed(index_range, start_vertex, instance_range);
    }

    /// Encode the necessary commands to render the contents of the given **Draw**ing to the given
    /// **Texture**.
    pub fn render_to_texture<S>(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        draw: &draw::Draw<S>,
        texture: &wgpu::Texture,
    ) where
        S: BaseFloat,
    {
        let size = texture.size();
        let view = texture.create_default_view();
        // TODO: Should we expose this for rendering to textures?
        let scale_factor = 1.0;
        let resolve_target = None;
        self.encode_render_pass(
            device,
            encoder,
            draw,
            scale_factor,
            size,
            &view,
            resolve_target,
        );
    }

    /// Encode the necessary commands to render the contents of the given **Draw**ing to the given
    /// **Frame**.
    pub fn render_to_frame<S>(
        &mut self,
        device: &wgpu::Device,
        draw: &draw::Draw<S>,
        scale_factor: f32,
        frame: &Frame,
    ) where
        S: BaseFloat,
    {
        let size = frame.texture().size();
        let attachment = frame.texture_view();
        let resolve_target = None;
        let mut command_encoder = frame.command_encoder();
        self.encode_render_pass(
            device,
            &mut *command_encoder,
            draw,
            scale_factor,
            size,
            attachment,
            resolve_target,
        );
    }
}

fn create_depth_texture(
    device: &wgpu::Device,
    size: [u32; 2],
    depth_format: wgpu::TextureFormat,
    sample_count: u32,
) -> wgpu::Texture {
    wgpu::TextureBuilder::new()
        .size(size)
        .format(depth_format)
        .usage(wgpu::TextureUsage::OUTPUT_ATTACHMENT)
        .sample_count(sample_count)
        .build(device)
}

fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    wgpu::BindGroupLayoutBuilder::new().build(device)
}

fn bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> wgpu::BindGroup {
    wgpu::BindGroupBuilder::new().build(device, layout)
}

fn render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    vs_mod: &wgpu::ShaderModule,
    fs_mod: &wgpu::ShaderModule,
    dst_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
    msaa_samples: u32,
) -> wgpu::RenderPipeline {
    wgpu::RenderPipelineBuilder::from_layout_descriptor(&[layout][..], vs_mod)
        .fragment_shader(fs_mod)
        .color_format(dst_format)
        .add_vertex_buffer::<Vertex>()
        .depth_format(depth_format)
        .sample_count(msaa_samples)
        .build(device)
}
