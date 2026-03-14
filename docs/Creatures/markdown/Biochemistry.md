# Biochemistry

A [creature's](https://creatures.wiki/Creature) **biochemistry** is the collection (sum total) of its [chemical reactions](https://creatures.wiki/Chemical_reaction). A realistic biochemistry with analagous [chemicals](https://creatures.wiki/Chemical), [emitters](https://creatures.wiki/Emitter), [neuroemitters](https://creatures.wiki/Neuroemitter), and [receptors](https://creatures.wiki/Receptor) to the real world is a big part of making creatures act realistically, because when their [brains](https://creatures.wiki/Brain) are linked up to chemicals, they can decide things like "maybe it would be a good idea to eat some [food](https://creatures.wiki/Food) when my [glycogen](https://creatures.wiki/Glycogen) level is low".

This is also a good place to look when creatures are behaving oddly, as a mutation may have caused a positive reinforcement loop in their brain regarding ideas like [walking into walls](https://creatures.wiki/Wallbonk).

In Creatures 2 and Creatures 3, some of the genes involved in biochemistry depend on the [organ](https://creatures.wiki/Organ) that contains those genes being alive, so [heavy metal](https://creatures.wiki/Heavy_metal) poisoning or other [diseases](https://creatures.wiki/Disease) can cause a creature to lose some important biochemical reactions.

## Chemicals

[Chemicals](https://creatures.wiki/Chemical) are the components of a [Creature](https://creatures.wiki/Creature)'s biochemistry. They can react with each other in [chemical reactions](https://creatures.wiki/Chemical_reaction) defined by the creature's [genetics](https://creatures.wiki/Genetics). The chemicals are arbitrary - they possess no innate qualities of their own, and what they do to a creature is solely determined by its genetics, including ratios.

There are complete lists of [Creatures 1 chemicals](https://creatures.wiki/C1_Chemical_List), [Creatures 2 chemicals](https://creatures.wiki/C2_Chemical_List), and [Creatures 3 chemicals](https://creatures.wiki/C3_Chemical_List). Some chemicals also have separate pages; these are in the [Chemicals category](https://creatures.wiki/Category:Chemicals).

### Initial chemical concentrations

In all games, initial chemical concentrations are set by genetics.

In [Creatures 1](https://creatures.wiki/Creatures_1), late-switching chemical concentrations genes will reset the chemical to the new value when the creature hits that life stage.

In [Creatures 2](https://creatures.wiki/Creatures_2), these late-switching genes are ignored.

In the [Creatures Evolution Engine](https://creatures.wiki/Creatures_Evolution_Engine), this late-switching gene behavior is restored, similar to Creatures 1.

### Half-lives

The **half-life** of any [chemical](https://creatures.wiki/Chemical), either in the [Creatures series](https://creatures.wiki/Creatures_series) or our own universe, is the time it takes for a given amount of any chemical to decay to half the original value. In Creatures, altering this can have profound effects on biochemistry.

In [creatures](https://creatures.wiki/Creatures), the half-life of any chemical is determined by a big [gene](https://creatures.wiki/Gene) called, oddly enough, the **half-life gene**. The half-life gene is the longest gene in a creature, as it [contains the decay rates of all the chemicals in a creature's biochemistry](https://web.archive.org/web/20170814231455/http://www.gamewareeurope.com/GWDev/cdn/CDN_more.php?CDN_article_id=9). A common mutation of the half-life gene produces a longer half-life for [ageing](https://creatures.wiki/Ageing) or [life](https://creatures.wiki/Life), leading to longer-lived [creatures](https://creatures.wiki/Creatures).

As of Creatures 2, if you [hex edit](https://creatures.wiki/Hex_edit) a gene file and change the decay rate to values that are not in the included table, you will just get the same behaviour as the nearest lower value in the table. For example, any value from 0 to 7 will behave as 0 does, and any value from 64-71 will behave the same as 64. [source](https://web.archive.org/web/20010718104712/http://cdn.creatures.net/c2/knowledge/genetics/c2_genetics_hl.shtm)

See [biological half-life](https://en.wikipedia.org/wiki/biological_half-life) on Wikipedia for more on the real-life topic.

### Reactions

**Chemical reactions** are [genes](https://creatures.wiki/Gene) which control the changing of one group of [chemicals](https://creatures.wiki/Chemical) to another. These reactions can be found in the creature's [Digital DNA](https://creatures.wiki/Digital_DNA) as part of their biochemistry. The main limitation on these is that it is not possible to create a reaction of the form **[Nothing]** --> **Chemical** (the reverse *is* possible). Some of these reactions can be [mutated](https://creatures.wiki/Mutation) to devastating effect, e.g. converting [energy](https://creatures.wiki/Energy) into [glycotoxin](https://creatures.wiki/Glycotoxin).

The basic form of a chemical reaction in the genes can be written as iA + jB → kC + lD at a given rate, where ijkl are ratios and ABCD are chemicals. In addition to this basic form, A + B → C ('fusion'), A → NONE (exponential decay), A + B → A + C (catalysis) and A + B → A (catalytic breakdown of B) are possible.

The rate at which reactions occur is concentration-dependent. [[1]](http://mrl.snu.ac.kr/courses/CourseSyntheticCharacter/grand96creatures.pdf)

As chemicals have no innate properties of their own, stoichiometry is entirely controlled by genetics - which can lead to energy being created from reducing sex drive, as in the [Bacchus](https://creatures.wiki/Bacchus) mutation, or large amounts of long-term energy being created from disproportionally small amounts of short-term energy, as in the [Highlander](https://creatures.wiki/Highlander) mutation.

As an example, consider the reaction `2H + O -> W`, and current chemical levels of `10H` and `10O`. Each biotick, the current levels of chemical H and chemical O are examined to determine if and how much of the reaction can occur. In this case, there are five units available for the reaction (`10H + 5O`). The number of units available is reduced by the reaction rate, in the same manner as a chemical half life, and the resulting number of units are then consumed by the reaction, and an equivalent number of units of output chemicals are created.

### Creatures 1 chemicals list

Please note that the numbers given relate to the numbers used in [C1 CAOS Codes](https://creatures.wiki/C1_CAOS_Codes) and [COBs](https://creatures.wiki/COB): they are represented in [hexadecimal](https://creatures.wiki/Hexadecimal) in [genomes](https://creatures.wiki/Genome).

## Emitters

An **emitter** (or **chemoemitter**) releases [chemicals](https://creatures.wiki/Chemical) into the "bloodstream" of a [creature](https://creatures.wiki/Creature), affecting its biochemistry. An emitter gene controls what chemical, how much, and under what circumstances.

One common [mutation](https://creatures.wiki/Mutation) in this gene in C1 was instead of an emitter emitting [DecASH](https://creatures.wiki/DecASH) all the time, it emitted [alcohol](https://creatures.wiki/Alcohol) instead, leading to a creature that was permanently drunk. [Slave](https://creatures.wiki/Slave) suffered from this mutation.

### Creatures 1 emitter processing

Every processing period (`sample rate * bioticks`) the specified locus of a given tissue of a given organ is examined to determine if and how much of a chemical should be released.

There are two types of calculations that can be done:

- Analog emitters (`!(flags & 2)`) release a chemical proportional to the signal level received, according to the calculation `(signal - threshold) * (gain / 255) if signal > threshold else 0`
- Digital emitters (`flags & 2`) release a chemical entirely when they see a certain signal level, according to the calculation `gain if signal > threshold else 0`

Additionally, emitters may reset a locus to zero when a signal level above threshold is seen (`flags & 1`); and may treat a locus signal as its inverted value, e.g. 255 would become 0 (`flags & 4`).

[Chris Double](https://creatures.wiki/Chris_Double) [notes that](http://double.nz/creatures/genetics/emitter.htm) "when a norn is born the emitter is processed at least twice. So even if the sample rate is set to almost never the emitter will be processed." and also "Sometimes the emitter is processed when importing a norn. A norn with an emitter set to almost never had the emitter processed when imported. Could this be related to import deaths in some way?"

### Creatures 1 emitter loci

## Receptors

**Receptors** monitor [chemical](https://creatures.wiki/Chemical) levels and change the [brain](https://creatures.wiki/Brain)'s behaviour - for example, shivering to relieve [coldness](https://creatures.wiki/Coldness). They are fed by [emitters](https://creatures.wiki/Emitter). One of the things that receptors do is monitor the [ageing](https://creatures.wiki/Ageing) or [life](https://creatures.wiki/Life) chemical and tell the norn when to change [life stages](https://creatures.wiki/Life_stage). In some instances, receptors may control the reaction rate of a chemical reaction.

In C3, receptors were updated to bind to the reaction rate locus as well as the organ clockrate locus.

### Creatures 1 receptor processing

Every biotick, the amount of the specified chemical is examined to determine the resulting value of the given locus.

If the chemical is above the threshold value, then the new value of the locus is calculated according to one of two rules:

- Analog receptors (`!(flags & 2)`) stimulate a locus proportional to the signal level received: `nominal + (chemical - threshold) * gain / 255 * R`
- Digital receptors (`flags & 2`) stimulate a locus when they see a certain chemical level: `nominal + gain * R`

In these calculations, `R` is 1 normally, or -1 if "Output REDUCES with increased stimulation" is set (`flags & 1`).

If the chemical is not above the threshold value, then the locus is just set to `nominal`.

### Creatures 1 receptor loci

## Stimuli

Chemicals are also affected directly by [stimuli](https://creatures.wiki/Stimulus). Agents, COBs, and other scripts may send stimulus messages to a creature (usually because the creature is interacting with it). Stimulus genes within the creature's [genome](https://creatures.wiki/Genome) define exactly how and which chemicals are altered in response.

See more on the [Stimulus](https://creatures.wiki/Stimulus) page.

## Organs

Creatures 2 diagram showing the different functions of the organs.**Organs** were introduced in [Creatures 2](https://creatures.wiki/Creatures_2).

They, like in real creatures, work together to ensure the creatures survival. 
Certain organs, like the heart, lungs, brain, and various support organs, are essential for life in the default genome. If any of these organs stop functioning, it can cause death!

Organs all have certain chemical reactions that occur within them, for example, 1 water + 1 nothing = 1 hotness decrease + 1 nothing. This is the reaction for sweating and takes place in the skin organ. If the skin organ were to stop functioning, the temperature of the creature would rise.

Organs have life forces and clock rates. The clock rate is how fast the reactions are taking place in that organ, while the lifeforce is the health of the organ. Organ lifeforce naturally decays over time as the creature ages, but certain chemicals (Antigens, [Lactate](https://creatures.wiki/Lactate), [heavy metals](https://creatures.wiki/Heavy_metals)) can cause the lifeforce of certain organs to deteriorate faster. Another major lifeforce killer is ATP deficiency, which causes all organs to deteriorate very quickly. As of C2, each organ comes with a certain ATP cost, and if an organ does not have enough ATP, it loses lifeforce until its needs are met. [source](https://groups.google.com/forum/#!searchin/alt.games.creatures/uterus$20c2/alt.games.creatures/9zfViBmUrbs/aN80MRC-SkUJ)

The brain decreases to low lifeforce in seconds.

Organs naturally become less functional over the lifespan of the creature - this is controlled by a setting called 'organ vulnerability'. [source](http://www.creaturesvillage.com/creatures1/library/science/gen_tut/sci_genkit_tut7.htm)

According to [Verm](https://creatures.wiki/User:Verm), in C3/DS, organs all start out with the maximum of lifeforce, despite there being settings in the genome for less than perfect organ health at birth, because code is reused from Creatures 2.

There are lists of [C2 Organs](https://creatures.wiki/C2_Organs) and [C3 Organs](https://creatures.wiki/C3_Organs).

## Neuroemitters

Starting in the [Creatures Evolution Engine](https://creatures.wiki/Creatures_Evolution_Engine), creatures can have defined **neuroemitters**. Like an [emitter](https://creatures.wiki/Emitter), it gives a small amount of four chemicals. The neuroemitter is triggered by [neurons](https://creatures.wiki/Neuron), rather than locus levels. The sole neuroemitter in the standard C3 norn gives adrenalin, fear, and crowded when the norn sees a grendel.

## See also

- [Biochemistry Set](https://creatures.wiki/Biochemistry_Set) (C3/DS)

## External links

- [Creatures: Artificial Life Autonomous Software Agents for Home Entertainment](http://citeseerx.ist.psu.edu/viewdoc/download?doi=10.1.1.81.1278&rep=rep1&type=pdf) by [Steve Grand](https://creatures.wiki/Steve_Grand) et al.
- [A good illustrational overview of creature biochemistry](https://web.archive.org/web/20170814211919/http://www.gamewareeurope.com/GWDev/images/uploads/biochem.htm)
- [Norn Biochemistry 101: A peek inside a Norn](https://web.archive.org/web/20170627125417/http://www.gamewareeurope.com:80/GWDev/creatures_more.php?id=460_0_6_0_M27) - the nitty gritty
- [GEN File Format - Biochemistry](https://web.archive.org/web/20170814231455/http://www.gamewareeurope.com/GWDev/cdn/CDN_more.php?CDN_article_id=9) at the CDN
- [Genetics Lesson: Examining a Creatures Half-Life](https://web.archive.org/web/20201028174013/https://discoveralbia.com/2013/01/genetics-lesson-examining-a-creatures-half-life.html) at [Discover Albia](https://creatures.wiki/Discover_Albia) - contains a table of half-life numerical values compared to real-world time for C1, C2 and C3/DS.
- [Half-Lives and Other Genetic Mysteries](https://naturingnurturing.blogspot.com/2017/11/half-lives-and-other-genetic-mysteries.html) at [Naturing::Nurturing](https://creatures.wiki/Naturing::Nurturing) - critiques the official [Genetics Kit](https://creatures.wiki/Genetics_Kit)'s information about halflife times.
- [How to Add Chemicals to the Science Kit in C1](https://malkinslittlecreaturesblog.blogspot.com/2016/05/how-to-add-chemicals-to-science-kit-in.html) by [Malkin](https://creatures.wiki/Malkin).
- [Chemical emitter](http://double.co.nz/creatures/genetics/emitter.htm) at the [Creatures Developer Resource](https://creatures.wiki/Creatures_Developer_Resource)
- [Receptor](http://www.double.co.nz/creatures/genetics/receptor.htm) at the [Creatures Developer Resource](https://creatures.wiki/Creatures_Developer_Resource)
- [How Hunger Works in Creatures 1](https://web.archive.org/web/20201029013408/https://discoveralbia.com/2015/08/how-hunger-works-in-creatures-1.html) at [Discover Albia](https://creatures.wiki/Discover_Albia) - contains an example of how emitters, receptors, and reactions work together
