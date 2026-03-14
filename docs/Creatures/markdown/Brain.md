# Brain

The brain in [Creatures](https://creatures.wiki/Creatures) is a very complex thing built of about 10 *lobes*, maybe 900 *neurons* and many thousands of *dendrites*. The brain works with the [creature's](https://creatures.wiki/Creature) [biochemistry](https://creatures.wiki/Biochemistry) in order to keep itself alive.

## Design

According to [Creatures: Artificial Life Autonomous Software Agents for Home Entertainment](https://creatures.wiki/Creatures:_Artificial_Life_Autonomous_Software_Agents_for_Home_Entertainment), issues that needed to be factored into the design of the brain included:

- It needed to be very efficient to run a population of thinking creatures on the home computer of the mid-1990s.
- There was a need to be able to run the brains of the type engineered for the first generation of creatures.
- There needed to be allowance for running many other possible brain types besides the typical one given to the first generation.
- It needed to be robust enough to allow for [mutation](https://creatures.wiki/Mutation) and there should be a decent possibility that later generations have equal or better brains than the first generation.

## Lobes

A **lobe** is a part of the brain that is dedicated to a certain function. Each lobe has an x, y, width and height coordinate, giving it a unique position in the grid of the brain.

A lobe contains both neurons and dendrites connecting to neurons in other lobes.

In C1 and C2, each lobe may only contain two types of dendrites, where each type can connect to a single other lobe. This allows each lobe of the brain to be connected to two other lobes.

In the [Creatures Evolution Engine](https://creatures.wiki/Creatures_Evolution_Engine), each lobe groups dendrites into arbitrary **tracts**, which allow more complex connections between lobes.

As of C1, the maximum number of neurons a lobe can have is 1024 (such as a 32 x 32 square).

For articles on particular lobes, see the [Brain category](https://creatures.wiki/Category:Brain)

### Multi-lobed

**Multi-lobed** [creatures](https://creatures.wiki/Creature) are those with an abnormally high brain [lobe](https://creatures.wiki/Lobe) count for their [species](https://creatures.wiki/Species) and/or game. Multi-lobedness occurs when a new brain lobe gene is formed, though many are simple duplications and so they occupy the same space in the brain and don't make the creature any smarter. In [Creatures 1](https://creatures.wiki/Creatures_1), where the normal brain lobe count is 9, creatures have been reported to have lobe counts of 36 or more. This mutation can occur in Creatures 1 or 2, but Creatures 3 norns' brains do not mutate.

### Creatures 1 lobe list

The lobes in a [Creatures 1](https://creatures.wiki/Creatures_1) [norn](https://creatures.wiki/Norn) are as follows (this also applies to most of the other [Cyberlife](https://creatures.wiki/Cyberlife)/[Creature Labs](https://creatures.wiki/Creature_Labs)-provided creatures):

## Neurons

A **neuron** is a place where you can store a number value. Most neurons lose the saved value over time, but some do this faster than others.

Neurons live in lobes, and are connected to other neurons via dendrites.

## Dendrites

**Dendrites** are the connections between different neurons. Dendrites work unidirectionally (one-way), so the value of neuron 1 may influence the value of neuron 2, but not vice versa. Dendrites may have different behaviours and some just transfer the value of one neuron to another one, while others may negate the value or do even more complex work. See also [concept lobe](https://creatures.wiki/Concept_lobe).

### Type 0 and Type 1

In [Creatures 1](https://creatures.wiki/Creatures_1) and [Creatures 2](https://creatures.wiki/Creatures_2), dendrites live in lobes, and may be either type 0 or type 1. Each dendrite type within a lobe may connect to a single other lobe, allowing each lobe of the brain to be connected to maximum two other lobes.

### Tracts

**Tracts** were introduced in the [Creatures Evolution Engine](https://creatures.wiki/Creatures_Evolution_Engine) as a new object for managing dendrites. Each tract defines a set of dendrites connecting between two lobes. This allowed brains to be more complex compared to [Creatures 1](https://creatures.wiki/Creatures_1) and [Creatures 2](https://creatures.wiki/Creatures_2), in which each lobe could only receive inputs from two other lobes.

## Interaction with biochemistry

Creatures are complex systems, with numerous interactions back and forth between the brain and [the biochemical system](https://creatures.wiki/Biochemistry).

### Chemical emitters

**[Emitters](https://creatures.wiki/Biochemistry#Emitters)** (or **chemoemitters**) release chemicals into the bloodstream of a creature based on the value of **emitter loci**. The first set of emitter loci allow chemicals to be released based on the current state of activity in the brain.

For instance, in [Creatures 1](https://creatures.wiki/Creatures_1), emitter organ 0 contains all loci corresponding to the brain, and allows releasing chemicals based on: activity in a certain lobe, number of loose type0 dendrites in a lobe, number of loose type1 dendrites in a lobe, or output of a specific neuron.

For more information on emitters, see the [Biochemistry](https://creatures.wiki/Biochemistry#Emitters) article.

### Chemical receptors

**[Receptors](https://creatures.wiki/Biochemistry#Receptors)** monitor chemical levels and may alter the brain's behavior in response—for example, shivering to relieve coldness. In Creatures 1, emitter organ 0 corresponds to the brain, and chemicals may affect many properties, including threshold, leakage, rest state, susceptibility, and more.

For more information on receptors, see the [Biochemistry](https://creatures.wiki/Biochemistry#Receptors) article.

### Neuroemitters

Starting in the [Creatures Evolution Engine](https://creatures.wiki/Creatures_Evolution_Engine), creatures can have defined **[neuroemitters](https://creatures.wiki/Biochemistry#Neuroemitters)**. Like an emitter, it gives a small amount of four chemicals. The neuroemitter is triggered by neurons, rather than locus levels. The sole neuroemitter in the standard C3 norn gives adrenalin, fear, and crowded when the norn sees a grendel.

For more information on neuroemitters, see the [Biochemistry](https://creatures.wiki/Biochemistry#Neuroemitters) article.

### Brain organ

In the games after [C2](https://creatures.wiki/C2), one way in which creatures can die is by the lifeforce of the brain [organ](https://creatures.wiki/Organ) - which contains only the brain lobes - becoming too low. This happens when the organ receives sufficient damage which, due to the lack of other biochemistry inside the organ, only happens when a creature has insufficient [ATP](https://creatures.wiki/ATP). The gene which governs this organ has a similar [structure](https://web.archive.org/web/20170814231455/http://www.gamewareeurope.com/GWDev/cdn/CDN_more.php?CDN_article_id=9) to other organ genes.

## SVRules

Creature brains use a neuronal processing system called **State Value Rules**, commonly shortened to **SVRules**.

These were fairly simple in [Creatures 1](https://creatures.wiki/Creatures_1) and [Creatures 2](https://creatures.wiki/Creatures_2), essentially designed to let you perform simple manipulations upon Creature variables (ie, biochemistry and brain values), with the possibility of adding conditions based upon these variables.

In [Creatures Evolution Engine](https://creatures.wiki/Creatures_Evolution_Engine) games such as [Creatures 3](https://creatures.wiki/Creatures_3), each neuron and dendrite is a fully functional [register machine](https://en.wikipedia.org/wiki/Register_machine). That means that you have several registers (places where you can store something) and one special working register. The most important commands on a register machine are LOAD, which loads a value into the working register and STORE, which stores a value from it into one of the other registers.

Operations like ADD always use the value of the working register.
Look at the following example, which adds the value from register 1 to the one of register 0 and stores the new value in register 2:

- LOAD 0
- ADD 1
- STORE 2

The SVRules system adds some extensions to the usual register machine, like access to the [Creature](https://creatures.wiki/Creature)'s chemical system or to its reception.

Take your favourite [genetic editor](https://creatures.wiki/Category:Genetic_Editors) and take a look into one of the Brain [Lobe](https://creatures.wiki/Lobe) or Brain Tract [genes](https://creatures.wiki/Gene) to get a full (?) list of supported operations.

A good place to learn more about the brain is [The Creatures Developer Resource](https://creatures.wiki/The_Creatures_Developer_Resource)

### A note from the programmer

This SVRule system was designed to deal with an interesting problem: How could I specify arbitrary behaviours for my neurons in a way that evolution could freely change, without it generating endless syntax errors in the process? For instance, imagine that a mutation were to occur in a line of conventional C++ code, such as "for (i=0; i<num; i++)". Almost every possible mutation would render the code unreadable (e.g. "fxr (i-0; ibnum; i4+)") and the chances of a useful (or even viable) mutation would be extremely low. At worst the whole simulation would crash.

To solve this problem I designed the SVRule system in such a way that EVERY statement you can write in it is legal and meaningful, even if it is not biologically useful. If a token was originally the operand for a command, for example, and the command later mutated to one that didn't require operands, the token would now be interpreted instead as a new command or a variable. The details of this aren't very important, but I guess it's an interesting example of the ways in which biology differs from computer programming. Biology tends to have this sort of robustness built into the design.

—[Digitalgod](https://creatures.wiki/User:Digitalgod) 12:39, 4 Apr 2005 (EDT)

### Creatures 1 and Creatures 2 opcodes

### Creatures 3 and Docking Station opcodes

### Creatures 3 and Docking Station operand types

## External links

- [Creatures: Artificial Life Autonomous Software Agents for Home Entertainment](http://citeseerx.ist.psu.edu/viewdoc/download?doi=10.1.1.81.1278&rep=rep1&type=pdf) by [Steve Grand](https://creatures.wiki/Steve_Grand) et al.
- [I Am Ron's Brain](http://web.archive.org/web/20071208173623/fp.cyberlifersrch.plus.com/creaturesarchive/braindesc.htm)
- [The AI of Creatures](https://www.alanzucconi.com/2020/07/27/the-ai-of-creatures/) by [Alan Zucconi](https://creatures.wiki/Alan_Zucconi?action=edit&redlink=1)
- [Creatures 1 Brain spreadsheet](https://docs.google.com/spreadsheets/d/1vgoUpvlNYTv3pwZfzp6z9FUx6mB0dnDh0jcPB1h_Yp4/edit#gid=0) by [Alan Zucconi](https://creatures.wiki/Alan_Zucconi?action=edit&redlink=1)
- [An online brain viewer tool for C1 export files](https://ratshack.neocities.org/js/c1brainviewer/html/index.html)
- [State Variable Rules](http://double.nz/creatures/genetics/svrules.htm) at [The Creatures Developer Resource](https://creatures.wiki/The_Creatures_Developer_Resource)
- [Creatures 3 SV Rules](https://sites.google.com/site/ruratboy/creatures3svrules) at [Ratboy's Creatures3 Stuff](https://creatures.wiki/Ratboy%27s_Creatures3_Stuff?action=edit&redlink=1)
- [C2e Brains](https://web.archive.org/web/20090107005318/http://wiki.ccdevnet.org/index.php/C2e_Brains#SVRule_Opcodes) at the [CCDevNet](https://creatures.wiki/CCDevNet) wiki
- [creatures 1 brains](http://zenzoa.com/creatures/c1-brains.html) at [Home Sweet Albia](https://creatures.wiki/Home_Sweet_Albia)
- [Peeking inside the mind of a norn: what happens when we take parts away?](https://malkinslittlecreaturesblog.blogspot.com/2016/01/norns-are-so-complex-and-they-work-on.html) on [Malkin's Little Creatures Blog](https://creatures.wiki/Malkin%27s_Little_Creatures_Blog?action=edit&redlink=1).
