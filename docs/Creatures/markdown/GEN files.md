# GEN files

**.gen files** contain the genetic information for new [breeds](https://creatures.wiki/Breed) as well as individual [creatures](https://creatures.wiki/Creature), identified by the [moniker](https://creatures.wiki/Moniker).

## Format

C2 and newer genome files start with the three characters `dna`, followed by the (textual) number representing the genome file version - '2' for [Creatures 2](https://creatures.wiki/Creatures_2), and '3' for the [Creatures Evolution Engine](https://creatures.wiki/Creatures_Evolution_Engine).

The main part of the file (which C1-era files start with immediately) consists of any number of [genes](https://creatures.wiki/Gene), which consist of the four characters `gene`, a gene header and gene data, which differs depending on the contents of the header. There is more information about the differing formats of genes in the [gene types category](https://creatures.wiki/Category:Gene_types).

When all the genes have occurred, there is then an end-of-file marker consisting of the four characters `gend`.

All numbers in the file are in [little-endian](https://en.wikipedia.org/wiki/little-endian) form unless otherwise specified.

### Gene header

A gene header starts with the marker `gene` (to distinguish from the file end marker `gend`), followed by two 8-bit integers representing the gene type and the gene subtype.

There are then another four 8-bit integers, representing the gene identifier (for reference by [GNO files](https://creatures.wiki/GNO_files)), the generation number of the gene, the age at which the gene should be switched on, and the gene flags.

In version 2/3 genomes, there's then an 8-bit mutability weighting value, and in version 3 genomes, an 8-bit [variant](https://creatures.wiki/Variant) value, used in [Creatures Village](https://creatures.wiki/Creatures_Village) to turn on different genes for different species without needing separate genome files for each species.

## Genes

Each gene has a different data format. Each file version contains similar genes, but some things may be different. The below information may not be correct for all versions.

### Brain Lobe (type 0 0)

#### Version 1

**DendriteType:**

#### Version 3

### Brain Organ (type 0 1)

Exists in version 3.

### Brain Tract (type 0 2)

Exists in version 3.

### Biochemistry Receptor (type 1 0)

### Biochemistry Emitter (type 1 1)

### Biochemistry Reaction (type 1 2)

### Biochemistry Halflives (type 1 3)

### Biochemistry Initial Concentration (type 1 4)

### Biochemistry Neuroemitter (type 1 5)

Exists in version 3.

### Creature Stimulus (type 2 0)

### Creature Genus (type 2 1)

### Creature Appearance (type 2 2)

### Creature Pose (type 2 3)

Values may mutate outside of the valid range. When genes are processed, values are wrapped to the range 0x20–0x5A and then all non-valid pose characters are replaced with `X` (["keep the current pose"](https://creatures.wiki/Gait)).

### Creature Gait (type 2 4)

Pose values may mutate outside of the valid range (0–99), but are wrapped to valid values (`pose % 100`) when a creature has genes processed due to being born or changing life stage.

### Creature Instinct (type 2 5)

### Creature Pigment (type 2 6)

### Creature Pigment Bleed (type 2 7)

Exists in version 3.

### Creature Facial Expression (type 2 8)

Exists in version 3.

### Organ (type 3 0)

Only exists in versions 2 and 3.

## Related links

- [GNO files](https://creatures.wiki/GNO_files)
- [Genetics](https://creatures.wiki/Genetics)
- [Genetic Editors](https://creatures.wiki/Category:Genetic_Editors)

## External links

- [Creature Labs' GEN Technical Information](https://web.archive.org/web/20170927094028/http://www.gamewareeurope.com/GWDev/cdn/cdn_more.php?CDN_article_id=9) - This is known to be extremely inaccurate.
