use std::{
    error::Error,
    io::{self, Cursor, Read, Seek},
    ops::Sub,
};

use base64::prelude::*;
use binrw::{helpers::count, io::NoSeek, prelude::*};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use indexmap::{IndexMap, IndexSet};
use itertools::{Itertools, MinMaxResult};

use crate::types::{JavaString, Object, PackedPoint2};

#[binrw]
#[brw(big, magic = b"msch\x01")]
#[br(map_stream = make_stream)]
#[bw(map_stream = |s| NoSeek::new(ZlibEncoder::new(s, Compression::default())))]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Schematic {
    // FIXME: ew
    #[bw(map = |_| minmax_range::<_, i16>(tiles.iter().map(|t| t.position.x).minmax()) + 1)]
    width: i16,
    #[bw(map = |_| minmax_range::<_, i16>(tiles.iter().map(|t| t.position.y).minmax()) + 1)]
    height: i16,

    #[bw(assert(width <= 128 && height <= 128))] // FIXME: hack
    #[bw(try_calc = (tags.len() + if tags.contains_key("labels") { 0 } else { 1 }).try_into())]
    tags_count: i8,
    #[br(parse_with = count(tags_count as usize))]
    #[bw(write_with = write_tags, args(labels))]
    tags: IndexMap<JavaString, JavaString>,

    #[br(calc = calc_labels(&tags))]
    #[bw(ignore)]
    labels: Vec<String>,

    #[bw(try_calc = blocks.len().try_into())]
    blocks_count: u8,
    #[br(parse_with = count(blocks_count as usize))]
    #[bw(map = |v| v.iter().cloned().collect_vec())]
    blocks: IndexSet<JavaString>,

    #[bw(try_calc = tiles.len().try_into())]
    tiles_count: i32,
    #[br(count = tiles_count, args { inner: (&blocks,) })]
    #[bw(args(&self.blocks), assert(tiles.len() <= 128 * 128))]
    tiles: Vec<SchematicTile>,
}

impl Schematic {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read_base64<T>(input: T) -> Result<Self, Box<dyn Error>>
    where
        T: AsRef<[u8]>,
    {
        Ok(Self::read(&mut Cursor::new(
            BASE64_STANDARD.decode(input)?,
        ))?)
    }

    pub fn write_base64(&self) -> BinResult<String> {
        let mut cur = Cursor::new(vec![]);
        self.write(&mut cur)?;
        Ok(BASE64_STANDARD.encode(cur.into_inner()))
    }

    pub fn add_tile(&mut self, tile: SchematicTile) -> &mut Self {
        self.blocks.insert(tile.block.clone().into());
        self.tiles.push(tile);
        self
    }

    pub fn tiles(&self) -> &Vec<SchematicTile> {
        &self.tiles
    }

    pub fn tile_mut(&mut self, index: usize) -> Option<&mut SchematicTile> {
        self.tiles.get_mut(index)
    }
}

enum ResultReader<R> {
    Ok(R),
    Err(io::Error),
}

impl<R: Read> Read for ResultReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Ok(r) => r.read(buf),
            Self::Err(e) => Err(io::Error::new(e.kind(), e.to_string())),
        }
    }
}

impl<R: Seek> Seek for ResultReader<R> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        match self {
            Self::Ok(r) => r.seek(pos),
            Self::Err(e) => Err(io::Error::new(e.kind(), e.to_string())),
        }
    }
}

fn make_stream<S>(s: S) -> ResultReader<impl Read + Seek>
where
    S: Read + Seek,
{
    let mut buf = Vec::new();
    match ZlibDecoder::new(s).read_to_end(&mut buf) {
        Ok(_) => ResultReader::Ok(Cursor::new(buf)),
        Err(e) => ResultReader::Err(e),
    }
}

fn minmax_range<T, R>(mm: MinMaxResult<T>) -> R
where
    T: Sub + Clone,
    R: Default + From<<T as Sub>::Output>,
{
    match mm.into_option() {
        Some((min, max)) => (max - min).into(),
        None => R::default(),
    }
}

fn calc_labels(tags: &IndexMap<JavaString, JavaString>) -> Vec<String> {
    tags.get("labels")
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

#[binrw::writer(writer, endian)]
fn write_tags(tags: &IndexMap<JavaString, JavaString>, labels: &Vec<String>) -> BinResult<()> {
    let mut tags = tags.clone();
    tags.insert(
        "labels".into(),
        serde_json::to_string(&labels)
            .map_err(io::Error::from)?
            .into(),
    );

    for (key, value) in tags.iter() {
        key.write_options(writer, endian, ())?;
        value.write_options(writer, endian, ())?;
    }

    Ok(())
}

#[binrw]
#[brw(big, import(blocks: &IndexSet<JavaString>))]
#[derive(Debug, Clone, PartialEq)]
pub struct SchematicTile {
    #[bw(try_calc = block_to_index(block, blocks))]
    block_index: i8,
    #[br(try_calc = index_to_block(block_index, blocks))]
    #[bw(ignore)]
    pub block: String,
    pub position: PackedPoint2,
    pub config: Object,
    pub rotation: i8,
}

fn block_to_index(
    block: &String,
    blocks: &IndexSet<JavaString>,
) -> Result<i8, Box<dyn Error + Send + Sync>> {
    blocks
        .get_index_of(block)
        .map(|i| i as i8)
        .ok_or_else(|| format!("unknown block: {block}").into())
}

fn index_to_block(
    index: i8,
    blocks: &IndexSet<JavaString>,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let index = usize::try_from(index)?;
    blocks
        .get_index(index)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("index out of range: {index}").into())
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use velcro::map_iter_from;

    use crate::types::{ContentID, ContentType};

    use super::*;

    type TestResult = Result<(), Box<dyn Error>>;

    #[test]
    fn test_read_base64_switch_off() -> TestResult {
        assert_eq!(
            Schematic::read_base64(
                "bXNjaAF4nBXIUQqAIBAA0dkKgzpi9GEqtGBbpNH1y/l5MAjSM5g/EuNjzcgcUwm3XlVPA1z2W8qFblkFV16tYf+30JrgAzvqDyA="
            )?,
            *Schematic {
                tags: map_iter_from! {
                    "name": "unnamed",
                    "description": "",
                    "labels": "[]",
                }
                .collect(),
                width: 1,
                height: 1,
                ..Default::default()
            }
            .add_tile(SchematicTile {
                block: "switch".to_string(),
                position: PackedPoint2 { x: 0, y: 0 },
                config: false.into(),
                rotation: 0
            })
        );
        Ok(())
    }

    #[test]
    fn test_read_base64_switch_on() -> TestResult {
        assert_eq!(
            Schematic::read_base64(
                "bXNjaAF4nBXIUQqAIBAA0dkKgzpi9GEqtGBbpNH1y/l5MAjSM5g/EuNjzcgcUwm3XlVPA1z2W8qFblkFV16tYf+30JqEDzvsDyE="
            )?,
            *Schematic {
                tags: map_iter_from! {
                    "name": "unnamed",
                    "description": "",
                    "labels": "[]",
                }
                .collect(),
                width: 1,
                height: 1,
                ..Default::default()
            }
            .add_tile(SchematicTile {
                block: "switch".to_string(),
                position: PackedPoint2 { x: 0, y: 0 },
                config: true.into(),
                rotation: 0
            })
        );
        Ok(())
    }

    #[test]
    fn test_read_base64_sorters() -> TestResult {
        assert_eq!(
            Schematic::read_base64(
                "bXNjaAF4nDWIOw6AIBTACigODh7QOKC8gQTBAN7fX+zSphiUoUtuF4YzPfaMXupWwtFCToCNbpVY0fOisDWXJuXehh8F/Vf6rYkLXaIPQw=="
            )?,
            *Schematic {
                tags: map_iter_from! {
                    "name": "unnamed",
                    "description": "",
                    "labels": "[]",
                }
                .collect(),
                width: 3,
                height: 1,
                ..Default::default()
            }
            .add_tile(SchematicTile {
                block: "sorter".to_string(),
                position: PackedPoint2 { x: 0, y: 0 },
                config: Object::Null,
                rotation: 0,
            })
            .add_tile(SchematicTile {
                block: "sorter".to_string(),
                position: PackedPoint2 { x: 1, y: 0 },
                config: ContentID {
                    type_: ContentType::Item,
                    id: 0
                }
                .into(),
                rotation: 0,
            })
            .add_tile(SchematicTile {
                block: "sorter".to_string(),
                position: PackedPoint2 { x: 2, y: 0 },
                config: ContentID {
                    type_: ContentType::Item,
                    id: 15
                }
                .into(),
                rotation: 0,
            })
        );
        Ok(())
    }

    fn assert_roundtrip_base64(data: &str) -> TestResult {
        let want = BASE64_STANDARD.decode(data)?;
        let got = BASE64_STANDARD.decode(Schematic::read_base64(data)?.write_base64()?)?;

        // compare the plain part
        assert_eq!(want[..5], got[..5]);

        // zlib output doesn't seem to be reproducible, so decompress the compressed part
        let mut want_deflate = Vec::new();
        flate2::bufread::ZlibDecoder::new(&want[5..]).read_to_end(&mut want_deflate)?;
        let mut got_deflate = Vec::new();
        flate2::bufread::ZlibDecoder::new(&got[5..]).read_to_end(&mut got_deflate)?;
        assert_eq!(want_deflate, got_deflate);

        Ok(())
    }

    #[test]
    fn test_roundtrip_base64_switch_off() -> TestResult {
        assert_roundtrip_base64(
            "bXNjaAF4nBXIUQqAIBAA0dkKgzpi9GEqtGBbpNH1y/l5MAjSM5g/EuNjzcgcUwm3XlVPA1z2W8qFblkFV16tYf+30JrgAzvqDyA=",
        )
    }

    #[test]
    fn test_roundtrip_base64_switch_on() -> TestResult {
        assert_roundtrip_base64(
            "bXNjaAF4nBXIUQqAIBAA0dkKgzpi9GEqtGBbpNH1y/l5MAjSM5g/EuNjzcgcUwm3XlVPA1z2W8qFblkFV16tYf+30JqEDzvsDyE=",
        )
    }

    #[test]
    fn test_roundtrip_base64_sorters() -> TestResult {
        assert_roundtrip_base64(
            "bXNjaAF4nDWIOw6AIBTACigODh7QOKC8gQTBAN7fX+zSphiUoUtuF4YzPfaMXupWwtFCToCNbpVY0fOisDWXJuXehh8F/Vf6rYkLXaIPQw==",
        )
    }

    #[test]
    fn test_roundtrip_struct_switch_off() -> TestResult {
        let schem = Schematic::new()
            .add_tile(SchematicTile {
                block: "switch".to_string(),
                position: PackedPoint2 { x: 0, y: 0 },
                config: false.into(),
                rotation: 0,
            })
            .to_owned();
        let mut cur = Cursor::new(vec![]);
        schem.write(&mut cur)?;
        cur.set_position(0);
        assert_eq!(
            Schematic::read(&mut cur)?,
            Schematic {
                tags: map_iter_from! { "labels": "[]" }.collect(),
                width: 1,
                height: 1,
                ..schem
            }
        );
        Ok(())
    }

    #[test]
    fn test_roundtrip_struct_switch_on() -> TestResult {
        let schem = Schematic::new()
            .add_tile(SchematicTile {
                block: "switch".to_string(),
                position: PackedPoint2 { x: 0, y: 0 },
                config: true.into(),
                rotation: 0,
            })
            .to_owned();
        let mut cur = Cursor::new(vec![]);
        schem.write(&mut cur)?;
        cur.set_position(0);
        assert_eq!(
            Schematic::read(&mut cur)?,
            Schematic {
                tags: map_iter_from! { "labels": "[]" }.collect(),
                width: 1,
                height: 1,
                ..schem
            }
        );
        Ok(())
    }

    #[test]
    fn test_roundtrip_struct_sorters() -> TestResult {
        let schem = Schematic::new()
            .add_tile(SchematicTile {
                block: "sorter".to_string(),
                position: PackedPoint2 { x: 0, y: 0 },
                config: Object::Null,
                rotation: 0,
            })
            .add_tile(SchematicTile {
                block: "sorter".to_string(),
                position: PackedPoint2 { x: 1, y: 0 },
                config: ContentID {
                    type_: ContentType::Item,
                    id: 0,
                }
                .into(),
                rotation: 0,
            })
            .add_tile(SchematicTile {
                block: "sorter".to_string(),
                position: PackedPoint2 { x: 2, y: 0 },
                config: ContentID {
                    type_: ContentType::Item,
                    id: 15,
                }
                .into(),
                rotation: 0,
            })
            .to_owned();
        let mut cur = Cursor::new(vec![]);
        schem.write(&mut cur)?;
        cur.set_position(0);
        assert_eq!(
            Schematic::read(&mut cur)?,
            Schematic {
                tags: map_iter_from! { "labels": "[]" }.collect(),
                width: 3,
                height: 1,
                ..schem
            }
        );
        Ok(())
    }
}
