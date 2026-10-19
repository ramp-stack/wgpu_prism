use wgpu::{
    COPY_BUFFER_ALIGNMENT, BufferDescriptor, BufferAddress, BufferUsages, Buffer, Device, Queue,
};

pub struct DynamicBufferDescriptor<'a> {
    pub label: Option<&'a str>,
    pub usage: BufferUsages,
}

pub struct DynamicBuffer {
    buffer: Buffer,
    size: BufferAddress,
    label: Option<String>,
}
impl DynamicBuffer {
    pub fn new(device: &Device, descriptor: &DynamicBufferDescriptor) -> Self {
        let size = 4096;
        DynamicBuffer {
            buffer: device.create_buffer(&BufferDescriptor {
                label: descriptor.label, size, usage: descriptor.usage, mapped_at_creation: false,
            }),
            size, label: descriptor.label.map(|s| s.to_string()),
        }
    }

    pub fn write_buffer(&mut self, device: &Device, queue: &Queue, contents: &[u8]) {
        let pad = (4 - contents.len() % 4) % 4;
        let contents = if pad != 0 {&[contents, &vec![0u8; pad]].concat()} else {contents};

        if self.size >= contents.len() as u64 {
            queue.write_buffer(&self.buffer, 0, contents);
        } else {
            let mask = COPY_BUFFER_ALIGNMENT - 1;
            let size = ((contents.len() as u64).next_power_of_two() + mask) & !mask;
            self.buffer = device.create_buffer(&BufferDescriptor {
                label: self.label.as_deref(), size, usage: self.buffer.usage(),
                mapped_at_creation: true,
            });
            self.buffer.slice(..).get_mapped_range_mut()[..contents.len()].copy_from_slice(contents);
            self.buffer.unmap();
            self.size = size;
        }
    }

}
impl AsRef<Buffer> for DynamicBuffer {
    fn as_ref(&self) -> &Buffer {&self.buffer}
}

pub struct QuadIndexBuffer {buffer: DynamicBuffer, quads: u32}
impl QuadIndexBuffer {
    pub fn new(device: &Device) -> Self {
        Self {
            buffer: DynamicBuffer::new(device, &DynamicBufferDescriptor {
                label: None, usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            }),
            quads: 0,
        }
    }

    pub fn prepare(&mut self, device: &Device, queue: &Queue, quads: u32) {
        if quads <= self.quads {return}
        let quads = quads.next_power_of_two();
        let indices: Vec<_> = (0..quads).flat_map(|quad| {
            let vertex = quad * 4;
            [vertex, vertex+1, vertex+2, vertex+1, vertex+2, vertex+3]
        }).collect();
        self.buffer.write_buffer(device, queue, bytemuck::cast_slice(&indices));
        self.quads = quads;
    }
}
impl AsRef<Buffer> for QuadIndexBuffer {
    fn as_ref(&self) -> &Buffer {self.buffer.as_ref()}
}
