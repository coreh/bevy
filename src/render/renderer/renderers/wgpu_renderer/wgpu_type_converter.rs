use crate::{
    core::Window,
    prelude::Color,
    render::{
        pass::{LoadOp, StoreOp},
        pipeline::{
            state_descriptors::{
                BlendDescriptor, BlendFactor, BlendOperation, ColorStateDescriptor, ColorWrite,
                CompareFunction, CullMode, DepthStencilStateDescriptor, FrontFace, IndexFormat,
                PrimitiveTopology, RasterizationStateDescriptor, StencilOperation,
                StencilStateFaceDescriptor,
            },
            BindType, InputStepMode, VertexAttributeDescriptor, VertexBufferDescriptor,
            VertexFormat,
        },
        render_resource::BufferUsage,
        texture::{
            AddressMode, Extent3d, FilterMode, SamplerDescriptor, TextureDescriptor,
            TextureDimension, TextureFormat, TextureUsage, TextureViewDimension,
        },
    },
};

impl From<VertexFormat> for wgpu::VertexFormat {
    fn from(val: VertexFormat) -> Self {
        match val {
            VertexFormat::Uchar2 => wgpu::VertexFormat::Uchar2,
            VertexFormat::Uchar4 => wgpu::VertexFormat::Uchar4,
            VertexFormat::Char2 => wgpu::VertexFormat::Char2,
            VertexFormat::Char4 => wgpu::VertexFormat::Char4,
            VertexFormat::Uchar2Norm => wgpu::VertexFormat::Uchar2Norm,
            VertexFormat::Uchar4Norm => wgpu::VertexFormat::Uchar4Norm,
            VertexFormat::Char2Norm => wgpu::VertexFormat::Char2Norm,
            VertexFormat::Char4Norm => wgpu::VertexFormat::Char4Norm,
            VertexFormat::Ushort2 => wgpu::VertexFormat::Ushort2,
            VertexFormat::Ushort4 => wgpu::VertexFormat::Ushort4,
            VertexFormat::Short2 => wgpu::VertexFormat::Short2,
            VertexFormat::Short4 => wgpu::VertexFormat::Short4,
            VertexFormat::Ushort2Norm => wgpu::VertexFormat::Ushort2Norm,
            VertexFormat::Ushort4Norm => wgpu::VertexFormat::Ushort4Norm,
            VertexFormat::Short2Norm => wgpu::VertexFormat::Short2Norm,
            VertexFormat::Short4Norm => wgpu::VertexFormat::Short4Norm,
            VertexFormat::Half2 => wgpu::VertexFormat::Half2,
            VertexFormat::Half4 => wgpu::VertexFormat::Half4,
            VertexFormat::Float => wgpu::VertexFormat::Float,
            VertexFormat::Float2 => wgpu::VertexFormat::Float2,
            VertexFormat::Float3 => wgpu::VertexFormat::Float3,
            VertexFormat::Float4 => wgpu::VertexFormat::Float4,
            VertexFormat::Uint => wgpu::VertexFormat::Uint,
            VertexFormat::Uint2 => wgpu::VertexFormat::Uint2,
            VertexFormat::Uint3 => wgpu::VertexFormat::Uint3,
            VertexFormat::Uint4 => wgpu::VertexFormat::Uint4,
            VertexFormat::Int => wgpu::VertexFormat::Int,
            VertexFormat::Int2 => wgpu::VertexFormat::Int2,
            VertexFormat::Int3 => wgpu::VertexFormat::Int3,
            VertexFormat::Int4 => wgpu::VertexFormat::Int4,
        }
    }
}

impl From<&VertexAttributeDescriptor> for wgpu::VertexAttributeDescriptor {
    fn from(val: &VertexAttributeDescriptor) -> Self {
        wgpu::VertexAttributeDescriptor {
            format: val.format.into(),
            offset: val.offset,
            shader_location: val.shader_location,
        }
    }
}

impl From<InputStepMode> for wgpu::InputStepMode {
    fn from(val: InputStepMode) -> Self {
        match val {
            InputStepMode::Vertex => wgpu::InputStepMode::Vertex,
            InputStepMode::Instance => wgpu::InputStepMode::Instance,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OwnedWgpuVertexBufferDescriptor {
    pub stride: wgpu::BufferAddress,
    pub step_mode: wgpu::InputStepMode,
    pub attributes: Vec<wgpu::VertexAttributeDescriptor>,
}

impl From<&VertexBufferDescriptor> for OwnedWgpuVertexBufferDescriptor {
    fn from(val: &VertexBufferDescriptor) -> OwnedWgpuVertexBufferDescriptor {
        let attributes = val
            .attributes
            .iter()
            .map(|a| a.into())
            .collect::<Vec<wgpu::VertexAttributeDescriptor>>();
        OwnedWgpuVertexBufferDescriptor {
            step_mode: val.step_mode.into(),
            stride: val.stride,
            attributes,
        }
    }
}

impl<'a> From<&'a OwnedWgpuVertexBufferDescriptor> for wgpu::VertexBufferDescriptor<'a> {
    fn from(val: &'a OwnedWgpuVertexBufferDescriptor) -> Self {
        wgpu::VertexBufferDescriptor {
            attributes: &val.attributes,
            step_mode: val.step_mode,
            stride: val.stride,
        }
    }
}

impl From<Color> for wgpu::Color {
    fn from(color: Color) -> Self {
        wgpu::Color {
            r: color.r as f64,
            g: color.g as f64,
            b: color.b as f64,
            a: color.a as f64,
        }
    }
}

impl From<BufferUsage> for wgpu::BufferUsage {
    fn from(val: BufferUsage) -> Self {
        wgpu::BufferUsage::from_bits(val.bits()).unwrap()
    }
}

impl From<LoadOp> for wgpu::LoadOp {
    fn from(val: LoadOp) -> Self {
        match val {
            LoadOp::Clear => wgpu::LoadOp::Clear,
            LoadOp::Load => wgpu::LoadOp::Load,
        }
    }
}

impl From<StoreOp> for wgpu::StoreOp {
    fn from(val: StoreOp) -> Self {
        match val {
            StoreOp::Clear => wgpu::StoreOp::Clear,
            StoreOp::Store => wgpu::StoreOp::Store,
        }
    }
}

impl From<&BindType> for wgpu::BindingType {
    fn from(bind_type: &BindType) -> Self {
        match bind_type {
            BindType::Uniform {
                dynamic,
                properties: _,
            } => wgpu::BindingType::UniformBuffer { dynamic: *dynamic },
            BindType::Buffer { dynamic, readonly } => wgpu::BindingType::StorageBuffer {
                dynamic: *dynamic,
                readonly: *readonly,
            },
            BindType::SampledTexture {
                dimension,
                multisampled,
            } => wgpu::BindingType::SampledTexture {
                dimension: (*dimension).into(),
                multisampled: *multisampled,
            },
            BindType::Sampler => wgpu::BindingType::Sampler,
            BindType::StorageTexture { dimension } => wgpu::BindingType::StorageTexture {
                dimension: (*dimension).into(),
            },
        }
    }
}

impl From<Extent3d> for wgpu::Extent3d {
    fn from(val: Extent3d) -> Self {
        wgpu::Extent3d {
            depth: val.depth,
            height: val.height,
            width: val.width,
        }
    }
}

impl From<TextureDescriptor> for wgpu::TextureDescriptor {
    fn from(texture_descriptor: TextureDescriptor) -> Self {
        wgpu::TextureDescriptor {
            size: texture_descriptor.size.into(),
            array_layer_count: texture_descriptor.array_layer_count,
            mip_level_count: texture_descriptor.mip_level_count,
            sample_count: texture_descriptor.sample_count,
            dimension: texture_descriptor.dimension.into(),
            format: texture_descriptor.format.into(),
            usage: texture_descriptor.usage.into(),
        }
    }
}

impl From<TextureViewDimension> for wgpu::TextureViewDimension {
    fn from(dimension: TextureViewDimension) -> Self {
        match dimension {
            TextureViewDimension::D1 => wgpu::TextureViewDimension::D1,
            TextureViewDimension::D2 => wgpu::TextureViewDimension::D2,
            TextureViewDimension::D2Array => wgpu::TextureViewDimension::D2Array,
            TextureViewDimension::Cube => wgpu::TextureViewDimension::Cube,
            TextureViewDimension::CubeArray => wgpu::TextureViewDimension::CubeArray,
            TextureViewDimension::D3 => wgpu::TextureViewDimension::D3,
        }
    }
}

impl From<TextureDimension> for wgpu::TextureDimension {
    fn from(dimension: TextureDimension) -> Self {
        match dimension {
            TextureDimension::D1 => wgpu::TextureDimension::D1,
            TextureDimension::D2 => wgpu::TextureDimension::D2,
            TextureDimension::D3 => wgpu::TextureDimension::D3,
        }
    }
}

impl From<TextureFormat> for wgpu::TextureFormat {
    fn from(val: TextureFormat) -> Self {
        match val {
            TextureFormat::R8Unorm => wgpu::TextureFormat::R8Unorm,
            TextureFormat::R8Snorm => wgpu::TextureFormat::R8Snorm,
            TextureFormat::R8Uint => wgpu::TextureFormat::R8Uint,
            TextureFormat::R8Sint => wgpu::TextureFormat::R8Sint,
            TextureFormat::R16Unorm => wgpu::TextureFormat::R16Unorm,
            TextureFormat::R16Snorm => wgpu::TextureFormat::R16Snorm,
            TextureFormat::R16Uint => wgpu::TextureFormat::R16Uint,
            TextureFormat::R16Sint => wgpu::TextureFormat::R16Sint,
            TextureFormat::R16Float => wgpu::TextureFormat::R16Float,
            TextureFormat::Rg8Unorm => wgpu::TextureFormat::Rg8Unorm,
            TextureFormat::Rg8Snorm => wgpu::TextureFormat::Rg8Snorm,
            TextureFormat::Rg8Uint => wgpu::TextureFormat::Rg8Uint,
            TextureFormat::Rg8Sint => wgpu::TextureFormat::Rg8Sint,
            TextureFormat::R32Uint => wgpu::TextureFormat::R32Uint,
            TextureFormat::R32Sint => wgpu::TextureFormat::R32Sint,
            TextureFormat::R32Float => wgpu::TextureFormat::R32Float,
            TextureFormat::Rg16Unorm => wgpu::TextureFormat::Rg16Unorm,
            TextureFormat::Rg16Snorm => wgpu::TextureFormat::Rg16Snorm,
            TextureFormat::Rg16Uint => wgpu::TextureFormat::Rg16Uint,
            TextureFormat::Rg16Sint => wgpu::TextureFormat::Rg16Sint,
            TextureFormat::Rg16Float => wgpu::TextureFormat::Rg16Float,
            TextureFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
            TextureFormat::Rgba8UnormSrgb => wgpu::TextureFormat::Rgba8UnormSrgb,
            TextureFormat::Rgba8Snorm => wgpu::TextureFormat::Rgba8Snorm,
            TextureFormat::Rgba8Uint => wgpu::TextureFormat::Rgba8Uint,
            TextureFormat::Rgba8Sint => wgpu::TextureFormat::Rgba8Sint,
            TextureFormat::Bgra8Unorm => wgpu::TextureFormat::Bgra8Unorm,
            TextureFormat::Bgra8UnormSrgb => wgpu::TextureFormat::Bgra8UnormSrgb,
            TextureFormat::Rgb10a2Unorm => wgpu::TextureFormat::Rgb10a2Unorm,
            TextureFormat::Rg11b10Float => wgpu::TextureFormat::Rg11b10Float,
            TextureFormat::Rg32Uint => wgpu::TextureFormat::Rg32Uint,
            TextureFormat::Rg32Sint => wgpu::TextureFormat::Rg32Sint,
            TextureFormat::Rg32Float => wgpu::TextureFormat::Rg32Float,
            TextureFormat::Rgba16Unorm => wgpu::TextureFormat::Rgba16Unorm,
            TextureFormat::Rgba16Snorm => wgpu::TextureFormat::Rgba16Snorm,
            TextureFormat::Rgba16Uint => wgpu::TextureFormat::Rgba16Uint,
            TextureFormat::Rgba16Sint => wgpu::TextureFormat::Rgba16Sint,
            TextureFormat::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
            TextureFormat::Rgba32Uint => wgpu::TextureFormat::Rgba32Uint,
            TextureFormat::Rgba32Sint => wgpu::TextureFormat::Rgba32Sint,
            TextureFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
            TextureFormat::Depth32Float => wgpu::TextureFormat::Depth32Float,
            TextureFormat::Depth24Plus => wgpu::TextureFormat::Depth24Plus,
            TextureFormat::Depth24PlusStencil8 => wgpu::TextureFormat::Depth24PlusStencil8,
        }
    }
}

impl From<TextureUsage> for wgpu::TextureUsage {
    fn from(val: TextureUsage) -> Self {
        wgpu::TextureUsage::from_bits(val.bits()).unwrap()
    }
}

impl From<&DepthStencilStateDescriptor> for wgpu::DepthStencilStateDescriptor {
    fn from(val: &DepthStencilStateDescriptor) -> Self {
        wgpu::DepthStencilStateDescriptor {
            depth_compare: val.depth_compare.into(),
            depth_write_enabled: val.depth_write_enabled,
            format: val.format.into(),
            stencil_back: (&val.stencil_back).into(),
            stencil_front: (&val.stencil_front).into(),
            stencil_read_mask: val.stencil_read_mask,
            stencil_write_mask: val.stencil_write_mask,
        }
    }
}

impl From<&StencilStateFaceDescriptor> for wgpu::StencilStateFaceDescriptor {
    fn from(val: &StencilStateFaceDescriptor) -> Self {
        wgpu::StencilStateFaceDescriptor {
            compare: val.compare.into(),
            depth_fail_op: val.depth_fail_op.into(),
            fail_op: val.fail_op.into(),
            pass_op: val.pass_op.into(),
        }
    }
}

impl From<CompareFunction> for wgpu::CompareFunction {
    fn from(val: CompareFunction) -> Self {
        match val {
            CompareFunction::Never => wgpu::CompareFunction::Never,
            CompareFunction::Less => wgpu::CompareFunction::Less,
            CompareFunction::Equal => wgpu::CompareFunction::Equal,
            CompareFunction::LessEqual => wgpu::CompareFunction::LessEqual,
            CompareFunction::Greater => wgpu::CompareFunction::Greater,
            CompareFunction::NotEqual => wgpu::CompareFunction::NotEqual,
            CompareFunction::GreaterEqual => wgpu::CompareFunction::GreaterEqual,
            CompareFunction::Always => wgpu::CompareFunction::Always,
        }
    }
}

impl From<StencilOperation> for wgpu::StencilOperation {
    fn from(val: StencilOperation) -> Self {
        match val {
            StencilOperation::Keep => wgpu::StencilOperation::Keep,
            StencilOperation::Zero => wgpu::StencilOperation::Zero,
            StencilOperation::Replace => wgpu::StencilOperation::Replace,
            StencilOperation::Invert => wgpu::StencilOperation::Invert,
            StencilOperation::IncrementClamp => wgpu::StencilOperation::IncrementClamp,
            StencilOperation::DecrementClamp => wgpu::StencilOperation::DecrementClamp,
            StencilOperation::IncrementWrap => wgpu::StencilOperation::IncrementWrap,
            StencilOperation::DecrementWrap => wgpu::StencilOperation::DecrementWrap,
        }
    }
}

impl From<PrimitiveTopology> for wgpu::PrimitiveTopology {
    fn from(val: PrimitiveTopology) -> Self {
        match val {
            PrimitiveTopology::PointList => wgpu::PrimitiveTopology::PointList,
            PrimitiveTopology::LineList => wgpu::PrimitiveTopology::LineList,
            PrimitiveTopology::LineStrip => wgpu::PrimitiveTopology::LineStrip,
            PrimitiveTopology::TriangleList => wgpu::PrimitiveTopology::TriangleList,
            PrimitiveTopology::TriangleStrip => wgpu::PrimitiveTopology::TriangleStrip,
        }
    }
}

impl From<FrontFace> for wgpu::FrontFace {
    fn from(val: FrontFace) -> Self {
        match val {
            FrontFace::Ccw => wgpu::FrontFace::Ccw,
            FrontFace::Cw => wgpu::FrontFace::Cw,
        }
    }
}

impl From<CullMode> for wgpu::CullMode {
    fn from(val: CullMode) -> Self {
        match val {
            CullMode::None => wgpu::CullMode::None,
            CullMode::Front => wgpu::CullMode::Front,
            CullMode::Back => wgpu::CullMode::Back,
        }
    }
}

impl From<&RasterizationStateDescriptor> for wgpu::RasterizationStateDescriptor {
    fn from(val: &RasterizationStateDescriptor) -> Self {
        wgpu::RasterizationStateDescriptor {
            front_face: val.front_face.into(),
            cull_mode: val.cull_mode.into(),
            depth_bias: val.depth_bias,
            depth_bias_slope_scale: val.depth_bias_slope_scale,
            depth_bias_clamp: val.depth_bias_clamp,
        }
    }
}

impl From<&ColorStateDescriptor> for wgpu::ColorStateDescriptor {
    fn from(val: &ColorStateDescriptor) -> Self {
        wgpu::ColorStateDescriptor {
            format: val.format.into(),
            alpha_blend: (&val.alpha_blend).into(),
            color_blend: (&val.color_blend).into(),
            write_mask: val.write_mask.into(),
        }
    }
}

impl From<ColorWrite> for wgpu::ColorWrite {
    fn from(val: ColorWrite) -> Self {
        wgpu::ColorWrite::from_bits(val.bits()).unwrap()
    }
}

impl From<&BlendDescriptor> for wgpu::BlendDescriptor {
    fn from(val: &BlendDescriptor) -> Self {
        wgpu::BlendDescriptor {
            src_factor: val.src_factor.into(),
            dst_factor: val.dst_factor.into(),
            operation: val.operation.into(),
        }
    }
}

impl From<BlendFactor> for wgpu::BlendFactor {
    fn from(val: BlendFactor) -> Self {
        match val {
            BlendFactor::Zero => wgpu::BlendFactor::Zero,
            BlendFactor::One => wgpu::BlendFactor::One,
            BlendFactor::SrcColor => wgpu::BlendFactor::SrcColor,
            BlendFactor::OneMinusSrcColor => wgpu::BlendFactor::OneMinusSrcColor,
            BlendFactor::SrcAlpha => wgpu::BlendFactor::SrcAlpha,
            BlendFactor::OneMinusSrcAlpha => wgpu::BlendFactor::OneMinusSrcAlpha,
            BlendFactor::DstColor => wgpu::BlendFactor::DstColor,
            BlendFactor::OneMinusDstColor => wgpu::BlendFactor::OneMinusDstColor,
            BlendFactor::DstAlpha => wgpu::BlendFactor::DstAlpha,
            BlendFactor::OneMinusDstAlpha => wgpu::BlendFactor::OneMinusDstAlpha,
            BlendFactor::SrcAlphaSaturated => wgpu::BlendFactor::SrcAlphaSaturated,
            BlendFactor::BlendColor => wgpu::BlendFactor::BlendColor,
            BlendFactor::OneMinusBlendColor => wgpu::BlendFactor::OneMinusBlendColor,
        }
    }
}

impl From<BlendOperation> for wgpu::BlendOperation {
    fn from(val: BlendOperation) -> Self {
        match val {
            BlendOperation::Add => wgpu::BlendOperation::Add,
            BlendOperation::Subtract => wgpu::BlendOperation::Subtract,
            BlendOperation::ReverseSubtract => wgpu::BlendOperation::ReverseSubtract,
            BlendOperation::Min => wgpu::BlendOperation::Min,
            BlendOperation::Max => wgpu::BlendOperation::Max,
        }
    }
}

impl From<IndexFormat> for wgpu::IndexFormat {
    fn from(val: IndexFormat) -> Self {
        match val {
            IndexFormat::Uint16 => wgpu::IndexFormat::Uint16,
            IndexFormat::Uint32 => wgpu::IndexFormat::Uint32,
        }
    }
}

impl From<SamplerDescriptor> for wgpu::SamplerDescriptor {
    fn from(sampler_descriptor: SamplerDescriptor) -> Self {
        wgpu::SamplerDescriptor {
            address_mode_u: sampler_descriptor.address_mode_u.into(),
            address_mode_v: sampler_descriptor.address_mode_v.into(),
            address_mode_w: sampler_descriptor.address_mode_w.into(),
            mag_filter: sampler_descriptor.mag_filter.into(),
            min_filter: sampler_descriptor.min_filter.into(),
            mipmap_filter: sampler_descriptor.mipmap_filter.into(),
            lod_min_clamp: sampler_descriptor.lod_min_clamp,
            lod_max_clamp: sampler_descriptor.lod_max_clamp,
            compare_function: sampler_descriptor.compare_function.into(),
        }
    }
}

impl From<AddressMode> for wgpu::AddressMode {
    fn from(val: AddressMode) -> Self {
        match val {
            AddressMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
            AddressMode::Repeat => wgpu::AddressMode::Repeat,
            AddressMode::MirrorRepeat => wgpu::AddressMode::MirrorRepeat,
        }
    }
}

impl From<FilterMode> for wgpu::FilterMode {
    fn from(val: FilterMode) -> Self {
        match val {
            FilterMode::Nearest => wgpu::FilterMode::Nearest,
            FilterMode::Linear => wgpu::FilterMode::Linear,
        }
    }
}

impl From<&Window> for wgpu::SwapChainDescriptor {
    fn from(window: &Window) -> Self {
        wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: window.width,
            height: window.height,
            present_mode: if window.vsync {
                wgpu::PresentMode::Vsync
            } else {
                wgpu::PresentMode::NoVsync
            },
        }
    }
}
