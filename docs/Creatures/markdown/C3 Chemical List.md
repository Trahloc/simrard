# C3 Chemical List

Information compiled from various sources, especially the original (but occasionally erroneous) [Full Chemical List](https://web.archive.org/web/20170814212412/http://www.gamewareeurope.com/GWDev/cdn/C3chemicalList.php).

It is important to note that the effect of a [chemical](https://creatures.wiki/Chemical) depends on your creature's [genome](https://creatures.wiki/Genome). The effects listed here apply to typical creatures, especially but not exclusively normal breeds of [Norns](https://creatures.wiki/Norn). Some creatures like [Toxic Norns](https://creatures.wiki/Toxic_Norn) will have completely different reactions to certain chemicals. It is possible for mutations to be introduced which cause an otherwise normal Norn to develop an unusual reaction to a chemical. This can cause positive effects like immunity to a disease or immortality, or it may cause negative effects such as death at birth. Therefore, the tables listed below can only be considered general guidelines, though they will hold most of the time.

NOTE: Do not be alarmed when chemicals marked "bad" appear in your creature's bloodstream, unless they are toxins. Many are normal, they just produce negative effects when injected. Chemicals marked "vital" are necessary to your creature's survival and you may need to inject them if they are lacking, but injecting them when there is already a good supply may have no effect. Also, be warned that the good/bad column does not take into account how your creature will feel, only how it affects the creature's health.

# Stimulus Chemicals

Stimulus genes use a different numbering system for chemicals than most biological systems. You can convert a stimulus chemical number to a biological chemical number using the following formula:

If the stim chem is 255, then the bio chem is 0. Otherwise, the bio chem is (stim chem + 148) % 256 + (1 if stim chem >= 108 else 0)

# Biological Chemicals

## Unknownase

## Digestive

## Movement

## Waste

## Respiratory

## Reproductive

### General

### Female

### Male

## Toxins

### Basic toxins

### Antigens

All antigens are the products of bacterial infections. They can cause organ damage; see [C3 Organs](https://creatures.wiki/C3_Organs) to see what antigens attack which organs. Antigens also produce other chemicals. Note that Ettins are immune to all antigens except 0 through 2, and Grendels are immune to all antigens except 6 and 7. Norns should be particularly wary of antigen 5 because it produces "wounded", which can cause death!

### Wounded

## Medicinal

## Immune system

Chemicals 102 through 109 are antibodies 0 through 7, corresponding to antigens 0 through 7. Your creature will produce these antibodies on its own, but you can inject them to help combat antigens. Antibodies will only fight the corresponding antigen, for example, if your creature has antigen 5, you must inject antibody 5. You can even inject antibodies before the creature comes across the antigens, so that they're immunized.

## Regulatory

## Drive backups

Sometimes drives need to be suppressed when a creature should have other priorities. You should not inject any of these chemicals, because drives produce stress.

## Drive chemicals

These represent your creature's drives. You should not inject any of these chemicals, because drives produce stress.

## CA smell gradient chemicals

These represent stimuli -- sights, smells, etc. Injecting them will not harm your creature, though they will probably confuse it.

## Stress chemicals

Produced when the corresponding drive is too high.

## Unknownases/Custom Chemicals

## Brain chemicals

These are used to control your creature's learning and navigation. It is not a good idea to inject any of these except for the unused chemicals.

## See also

- [Biochemistry Set](https://creatures.wiki/Biochemistry_Set)
- [Creatures Development Standards](https://creatures.wiki/Creatures_Development_Standards)
