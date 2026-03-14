S. GRAND AND D. CLIFF.

Autonomous Agents and Multi-Agent Systems, 100, 1–20 (1997)
© 1997 Kluwer Academic Publishers, Boston. Manufactured in The Netherlands.

---

# Creatures: Entertainment Software Agents with Artificial Life

**STEPHEN GRAND**  
steve.grand@cyberlife.co.uk  
CyberLife Technology Ltd, Quayside, Bridge Street, Cambridge CB5 8AB, UK.  

**DAVE CLIFF**  
davec@ai.mit.edu  
Artificial Intelligence Laboratory, Massachusetts Institute of Technology, 545 Technology Square, Cambridge MA 02139, USA.  

**Editor:** Nick Jennings

---

## Abstract
We present a technical description of Creatures, a commercial home-entertainment software package. Creatures provides a simulated environment in which exist a number of synthetic agents that a user can interact with in real-time. The agents (known as "creatures") are intended as sophisticated "virtual pets." The internal architecture of the creatures is strongly inspired by animal biology. Each creature has a neural network responsible for sensory-motor coordination and behavior selection, and an "artificial biochemistry" that models a simple energy metabolism along with a "hormonal" system that interacts with the neural network to model diffuse modulation of neuronal activity and staged ontogenetic development. A biologically inspired learning mechanism allows the neural network to adapt during the lifetime of a creature. Learning includes the ability to acquire a simple verb–object language.

Additionally, both the network architecture and details of the biochemistry for a creature are specified by a variable-length "genetic" encoding, allowing for evolutionary adaptation through sexual reproduction. Creatures, available on Windows95 platforms since late 1996, offers users an opportunity to engage with Artificial Life technologies. In addition to describing technical details, this paper concludes with a discussion of the scientific implications of the system.

**Keywords:** Artificial Life; Adaptive Behavior; Evolutionary Computation; Entertainment Software.

---

## 1. Introduction
Autonomous software agents have significant potential for application in the entertainment industry. In this paper (revised from Grand, Cliff & Malhotra, 1997), we discuss an interactive entertainment product based on agent techniques originally developed in Artificial Life and Adaptive Behavior research. The product, called Creatures, allows human users to interact in real-time with synthetic agents which inhabit a closed environment. The agents, known as "creatures," have artificial neural networks for sensory-motor control and learning, artificial biochemistries for energy metabolism and hormonal regulation of behavior, and both the network and the biochemistry are "genetically" specified to allow for the possibility of evolutionary adaptation through sexual reproduction.

Although it is a commercial product, we believe aspects of Creatures will be of interest to the science and engineering communities. This paper discusses the most significant aspects of the product relevant to autonomous agent researchers.

---

## 2. Background

### 2.1. Artificial Life and Adaptive Behavior
Over the last ten years, two distinct but closely related fields of scientific inquiry have emerged: Artificial Life, and Adaptive Behavior. Artificial Life research is commonly characterized as the study of artificial systems that exhibit life-like behaviors, viewing "life" as it occurs on planet earth (i.e., rooted in carbon-chain chemistry) as one instance from a space of possible living systems, thereby offering the possibility of non-carbon-chain living entities, some of which might be digital organisms existing in virtual spaces. Clearly, artificial life research has the potential to address a wide range of phenomena, from self-replicating molecules, through the emergence of single-celled and multi-celled life-forms, to the evolution of whole species of life-forms and the cultural and social dynamics that occur when evolving agents can learn from and/or communicate with each other. 

In contrast, adaptive behavior research is more clearly focused on the issue of studying autonomous agents, be they real biological agents (i.e., animals) or artificial autonomous agents, which are commonly referred to in the adaptive behavior literature as "animats." Animats may be autonomous mobile robots, or software agents in virtual spaces. The emphasis in Adaptive Behavior research is on the mechanisms by which agents can coordinate perception and action, without human intervention, for extended periods of time in order to survive in environments that are generally dynamic, unknown, uncertain, and unforgiving of mistakes. For popular overviews, see the books by Levy (1993), Kelly (1994) or Coveney and Highfield (1995). For more academic literature on artificial life and adaptive behavior, see the recent conference proceedings edited by: Brooks and Maes (1994); Cliff et al. (1994); Moran et al. (1995); Maes et al. (1996); Langton and Shimohara (1997); and Husbands and Harvey (1997).

As with artificial life, in adaptive behavior research there is a strong emphasis on modeling biological mechanisms, and on drawing inspiration from biology in the development of artificial systems. Many of the autonomous agents developed in adaptive behavior research use artificial neural networks as "controllers" for coordinating perception and action. For general background on neural networks, see Rumelhart and McClelland (1986) and Arbib (1995). Also, many studies address the issue of using ideas from biological evolution, in the form of genetic algorithms (see e.g., Goldberg (1989)) or genetic programming (see e.g., Koza (1992)). In both cases, aspects of the design of an agent (such as the values of certain parameters governing its structure or operation), are encoded as the "genetic material" or "genome" for the agent. A population of agents is created, each with initially random genomes. Each agent is evaluated, to assess its fitness: a measure of how well-suited it is to the intended task or environment. The better an agent's fitness, the more likely it is to be selected for reproduction. In reproduction, the genetic material for new "offspring" agents is created by combining and randomly altering material from the genomes of (fitter) parents (see Figure 1), and the newly-created agents replace other agents. This process of evaluation, selection, reproduction, and replacement continues for some period of time, and (if all is well) the peak or average fitness in the agent population increases. That is, designs more appropriate to the task or environment evolve, without direct human intervention. In this sense, artificial evolution can be viewed as a form of semi-automatic parallel stochastic search through a (potentially vast) space of possible designs.

---

### 2.2. Autonomous Agents for Entertainment
Here we briefly summarize work in Artificial Life and Adaptive Behavior research that is relevant to Creatures.

Seminal work by Reynolds (1987) established the possibility of using autonomous agents for behavioral animation, a technique which allows movie sequences showing behavior in synthetic agents to be produced with the human animator giving only broad "choreographic" commands, rather than detailed frame-by-frame pose specifications. Subsequent related projects, such as that by Terzopoulos et al. (1994), where faithful kinematic simulations of sh are modeled with impressive visual accuracy and considerable biological plausibility in the behavioral control, have shared with Reynolds' original work a reliance on skillful manual design of the agent's physical morphology, behavioral control mechanism, or both. This can often require a significant investment of skilled labor.

Maes (1995) reviews other entertainment-oriented academic research projects, noting that Bates' (1994) Woggles World was pioneering work, providing a virtual world inhabited with animal-like artificial agents (called "woggles") that the user could interact with via mouse and keyboard input to directly control the behavior of a specific woggle. Individual woggles could exhibit emotions that varied on the basis of internal needs. Although Creatures was developed independently of Bates's work, there are clear similarities at the conceptual level. For more recent work, see Loyall and Bates (1997). Other work published in the autonomous agents literature that is comparable to Creatures includes Hayes-Roth and van Gent (1997), and Lester and Stone (1997).

Faced with the difficult task of designing lifelike synthetic agents for entertainment applications, several researchers have drawn inspiration from biology. For example, Blumberg (1994,1996) developed a behavioral control mechanism inspired by
  findings in ethology (the science of animal behavior) which is used to control a
synthetic dog that inhabits a simulated 3D environment, interacting with a human
user and with other virtual agents and objects in the environment.

Other researchers have worked on developing techniques that reduce the reliance
on skilled labor by incorporating some type of automatic adaptation or learning
mechanism in the agent software. Reynolds (1994) explored the use of genetic pro-
gramming to develop control programs for synthetic agents moving in 2D worlds
with simpli ed kinematics. Sims (1994) employed similar arti cial evolution tech-
niques to develop both the physical morphology and the arti cial neural network
controllers for synthetic autonomous agents that inhabit a 3D world with realistic
kinematics.

---

## 3. Creatures
We introduce the Creatures environment in Section 3.1, followed by details of the
creatures' neural networks in Section 3.2. In Section 3.3 we describe the biochem-
istry of the creatures. The genetics, which determine the neural network and the
biochemistry of each creature, are described in Section 3.4.

### 3.1. Environment
The creatures inhabit a "2 21 -dimensional" world: effectively a 2D platform envi-
ronment with multi-plane depth cueing so that objects can appear, relative to the
user, to be in front of or behind one another. On a typical Windows95 system,
the world measures approximately 15 screens horizontally by 4 screens vertically,
with the window scrolling smoothly to follow a selected creature. Within the world
there are a number of objects that the creature can interact with in a variety of
ways. The system has been written using object-oriented programming techniques:
virtual objects in the world such as toys, food, etc. have scripts attached that de-
termine how they interact with other objects, including the creature agents and the
static parts of the environment. Some objects are "automated", such as elevators
which rise/fall when a button is pressed. Additional objects and environments may
be subsequently acquired (e.g., by downloading from a web-site) and added to the
world. A screen-shot showing a view of part of the world is shown in Figure 2

When the user's mouse pointer is anywhere within the environment window, the
pointer changes to an image of a human hand. The user can move objects in the
environment by picking them up and dropping them, and can attract the attention
of a creature by waving the hand in front of it, or by stroking it (which generates
a positive, "reward" reinforcement signal) or slapping it (to generate a negative,
"punishment" reinforcement signal).

A typical creature is shown in Figure 3. All creatures are bipedal, but minor
morphological details such as coloring and hair type are genetically speci ed. As
they grow older, the on-screen size of the creature increases, up until "maturity",
approximately one third of the way through their life. The life-span of each creature
is genetically in uenced: if a creature manages to survive to old age (measured in
game-hours) then senescence genes may become active, eventually killing the crea-
ture. The creature has simulated senses of sight, sound, and touch. All are modeled
using semi-symbolic approximation techniques. For example, the simulation of vi-
sion does not involve a simulation of optics or processing of retinal images. Rather,
if a certain object is within the line of sight of a creature, a neuron representing
the presence of that object in the visual field becomes active. Such approxima-
tions to the end-result of sensory processing are fairly common in neural network
research. Sounds attenuate over distance and are mued by any objects between
the creature and the sound-source. An object can only be seen if the creature's eyes
are pointing in its direction. There is also a simple focus-of-attention mechanism,
described further below.

Creatures can learn a simple verb-object language, either via keyboard input
from the user, or by playing on a teaching-machine in the environment, or from
interactions with other creatures in the environment.

On typical target platforms, up to ten creatures can be active at one time before
serious degradation of response-time occurs. The following sections describe in
more detail the neural network, biochemistry, and genetics for the creatures.

### 3.2. Neural Network
Each creature's brain is a heterogeneous neural network, sub-divided into objects
called `lobes', which de ne the electrical, chemical and morphological characteristics
of a group of cells. Cells in each lobe form connections with one or more of the
cells in up to two other source lobes to perform the various functions and sub-
functions of the net. Figure 4 shows a schematic of interconnections between lobes.
The network architecture was designed to be biologically plausible, and computable
from the `bottom-up', with very few top-down constructs.

In the initial generation, each creature's brain contains approximately 1,000 neu-
rons, grouped into 9 lobes, and interconnected through roughly 5,000 synapses.
However, all these parameters are genetically controlled and may vary during later
phylogenesis.

The structure of the neural architecture was designed to satisfy several criteria:
   It must be very computationally ecient (a world with ten creatures requires
    the processing of some 20,000 neurons and 100,000 synaptic connections every
    second, in addition to the load imposed by the display and the rest of the
    system).
   It must be capable of supporting the initial brain model, i.e. the neural con g-
    uration which controls the rst generation of creatures.
   It must be capable of expressing many other possible neural models, besides the
    initial one.
   It must not be too brittle: mutation and recombination should have a fair
    chance of constructing new systems of equal or higher utility than those of the
    parents.
  In Section 3.2.1 we describe the components of the neural networks, and in Sec-
tion 3.2.2 we explain how these components are organized to give the Creatures
brain model.

  3.2.1. Components All the neurons within a single lobe share the same charac-
teristics, but these characteristics can vary over a wide range of possibilities. Some
aspects of the neurons' dynamics are determined by simple scalar numeric parame-
ters, while others are de ned via relatively complex mathematical expressions. All
of these factors are controlled genetically during the construction of a lobe. The
parameters of a neuron are as follows:

  3.2.2. Brain Model The above architecture is a generalized engine for neuron-
like computation, whose circuitry can be de ned genetically. This section describes
the speci c organizational model which has been superimposed onto the system to
implement the rst generation of creatures. Figure 6 shows the arrangement of the
lobes in the Creatures brain model.
  Some of the neural circuits are devoted to relatively minor tasks. For example,
two lobes are used to implement an attention-directing mechanism. Stimuli arriving
from objects in the environment cause a particular cell to re in an input lobe (where
each cell represents a di erent class of object). These signals are mapped one-on-
one into an output lobe, which sums the intensity and frequency of those stimuli
over time. Simulated lateral inhibition allows these cells to compete for control of
the creature's attention. The creature's gaze (and therefore much of its sensory
apparatus) is xed on this object, and it becomes the recipient for any actions the
creature chooses to take. Such a mechanism limits creatures to \verb{object", as
opposed to \subject{verb{object" modes of thought, but serves to reduce sensory
and neural processing to acceptable levels, since the net need only consider one
object at a time.
  The bulk of the remaining neurons and connections make up three lobes: a `per-
ception' lobe, which combines several groups of sensory inputs into one place; a
large region known as Concept Space, in which event memories are laid down and
evoked; and a small but massively dendritic lobe called the Decision Layer, where
relationship memories are stored and action decisions get taken. The overall model
is behaviorist and based on reinforcement by drive reduction.
  Cells in Concept Space are simple pattern-matchers. Each has one to four den-
drites and computes its output by calculating the logical and function of the analog
signals on its inputs, which come via the Perception lobe from sensory systems.
Each therefore res when all of its inputs are ring. These cells are randomly wired
at birth, but seek out new patterns as they occur. Once a cell has committed to
a particular pattern, it remains connected until its dendrites' strengths all fall to
zero. A biochemical feedback loop and two SVRules attempt to maintain a pool
of uncommitted neurons while leaving `useful' (i.e. repeatedly reinforced) cells con-
nected for long periods. The Perception lobe has around 128 sensory inputs, and
so the total number of cells that would be required to represent all possible sensory
permutations of up to four inputs is unfeasibly large. This reinforcement, atrophy
and migration mechanism is designed to get round this problem by recording only
the portion of input space which turns out to be relevant. There are a number of
problems associated with this approach, but on the whole it works.
  The Decision layer comprises only 16 cells, each representing a single possible ac-
tion, such as \activate it", \deactivate it", \walk west", and so on, where \it" is the
currently attended-to object. The Decision neurons are highly dendritic and feed
from Concept Space. The dendrites' job is to form relationships between Concept
cells and actions, and to record in their synaptic weightings how appropriate each
action is in any given sensory circumstance.
  An SVRule on each dendrite decides the current synaptic `susceptibility', i.e.
sensitivity to modulation by reinforcers. This is raised whenever that dendrite is
conducting a signal to a cell and that cell is ring (i.e. the connection represents both
a `true' condition and also the current action). It then decays exponentially over
time. Synapses are therefore sensitized when they represent relationships between
current sensory schemata and the latest action decision, and remain sensitive for
a period in order to respond to any share of a more-or-less deferred reward or
punishment.
  There are not enough dendrites to connect every action to every Concept cell, and
so these dendrites are also capable of migrating in search of new sources of signal.
Again a biochemical feedback loop controls atrophy, while repeated reinforcement
raises strength.
  Decision cells sum their inputs into their current state (in fact they sum their type
0 inputs (excitatory) and subtract the sum of their type 1 (inhibitory) inputs). The
relaxation rate of Decision cells is moderate, and so each cell accumulates a number
of nudges over a short period, based on the number of Concept cells which are ring,
plus their intensity. The strongest- ring Decision cell is taken to be the best course
of action, and whenever the winner changes, the creature invokes the appropriate
action script.
  The neural network includes mechanisms for generalization. Because Concept
Space seeks to represent all the various permutations of one to four inputs that
exist within the total sensory situation obtaining at a given moment, the system
is capable of generalizing from previously learned relationships to novel situations.
Two sensory situations can be deemed related if they share one or more individ-
ual sensory features, for example situation ABCD, which may never before have
been experienced, may evoke memories of related situations such as D, ABD, etc.
(although not BCDE). Each of these sub-situations represents previously learned
experience from one or more related situations and so each can o er useful advice
on how to react to the new situation. For example, \I nd myself looking at a big,
green thing with staring eyes, which I've never seen before. I remember that going up
to things with staring eyes and kissing them is not a good idea, and that hitting big
things, particularly big, green things, doesn't work well either. So, all in all, I think
I'll try something else this time." Of course, if the new situation turns out to have
different qualities from previously experienced sub-situations (an `exception to the
rule'), then both the new total `concept' and the previously learned sub-concepts
will be reinforced accordingly. As long as super-concepts re more strongly than
sub-concepts, and as long as reinforcement is supplied in proportion to cell output,
the creature can gradually learn to discriminate between these acquired memories
and so form ever more useful generalizations for the future.
  Delayed-reinforcement learning is provided by changes to Decision Layer short-
term weights in response to the existence of either a Reward chemical (for excitatory
synapses) or a Punishment chemical (for inhibitory ones). These chemicals are not
generated directly by environmental stimuli but during chemical reactions involved
in drive-level changes. Each creature maintains a set of chemicals representing
`drives', such as \the drive to avoid pain", \the drive to reduce hunger", and so
on. The higher the concentration of each chemical, the more pressing that drive.
Environmental stimuli cause the production of one or more drive raisers or drive
reducers: chemicals which react to increase or decrease the levels of drives. For
example, if the creature takes a shower by activating a shower object, the shower
might respond by reducing \hotness" and \coldness" (normalizing temperature),
decreasing tiredness and increasing sleepiness. Drive raisers and reducers produce
Punishment and Reward chemicals respectively through the reactions:

   DriveRaiser ) Drive + Punishment
   DriveReducer + Drive ) Reward

  Drive reduction therefore increases the weights of excitatory synapses while drive
increase reinforces inhibitory ones. Of course, reducing a non-present drive has
no e ect, and so the balance of punishment to reward may reverse. Thus, many
actions on objects can return a net punishment or a net reward, according to the
creature's internal state at the time. Creatures therefore learn to eat when hungry
but not when full.
  The brain model is not an ambitious one, and severely limits the range of cognitive
functions which can arise. It is also primitively Behaviorist in its reinforcement
mechanism. However, it serves its purpose by providing a learned logic for how
a creature chooses its actions, and doesn't su er from too many non-life-like side
e ects: its in-built generalization mechanism reduces arbitrariness in the face of
novelty; and the dynamical structure, albeit damped and close to equilibrium,
produces a satisfactorily complex and believable sequence of behaviors, surprisingly
free from limit cycles (e.g., repeatedly cycling through a xed sequence of actions)
or irretrievable collapse into point attractors (\grinding to a halt"). Determining
why the dynamics of such neural networks are stable is challenging issue, and a
topic of current research (see, e.g., Beer 1995a, 1995b, 1996).

### 3.3. Biochemistry

Central to the function of the neural net is the use of a simpli ed, simulated bio-
chemistry to control widespread information ow, such as internal feedback loops
and the external drive-control system. This mechanism is also used to simulate
other endocrine functions outside the brain, plus a basic metabolism and a very
simple immune system. The biochemistry is very straightforward and is based on
four classes of object: chemicals; emitters; reactions; and receptors. Combinations
of these objects form biochemical structures.

  3.3.1. Chemicals These are just arbitrary numeric labels in the range 0 to 255,
each representing a di erent chemical and each associated with a numeric value
representing its current concentration. Chemicals have no inherent properties: the
reactions which each can undergo are de ned genetically, with no restrictions based
on any in-built chemical or physical characteristics of the molecules themselves.

  3.3.2. Emitters These chemicals are produced by chemo-emitter objects, which
are genetically de ned and can be attached to arbitrary byte values within other
system objects, such as neurons in the brain or the outputs of sensory systems.
The locus of attachment is de ned by a descriptor at the start of an emitter gene,
representing `organ', `tissue' and `site', followed by codes for the chemical to be
emitted and the gain and other characteristics of the emitter. Changes in the value
of a byte to which an emitter is attached will automatically cause the emitter to
adjust its output, without the code which has caused the change needing to be
aware of the emitter's existence.

  3.3.3. Reactions Chemicals undergo transformations as de ned by Reaction
objects, which specify a reaction in the form iA + [jB ] ) [kC ] + [1D] where i; j;
and k determine ratios and optional components are enclosed in brackets. Most
transformations are allowed, except for nothing ) something, for example:
     A+B )C +D        Normal reaction with two products
     A+B )C           `fusion'
     A ) nothing      exponential decay
     A+B )A+C         catalysis (A is unchanged)
     A+B )A           catalytic breakdown (of B )
  Reactions are not de ned by immutable chemical laws but by genes, which specify
the reactants and reaction products and their proportions, along with a value for
the reaction rate, which is concentration-dependent and therefore exponential over
time.

  3.3.4. Receptors Chemical concentrations are monitored by chemo-receptor
objects, which attach to and set arbitrary bytes de ned by locus IDs, as for emit-
ters. Receptor genes specify the locus, the chemical that the receptor responds to,
the gain, the threshold and the nominal output. Many parts of the brain and body
can have receptors attached, and thus can become responsive to chemical changes.

  3.3.5. Biochemical structures Attaching receptors and emitters to various loci
within brain lobes allows widespread feedback paths within the brain, particularly
in combination with reactions. Paths have been implemented to control synaptic
atrophy and migration, and also to provide drive-reduction and learning reinforce-
ment. Other neurochemical interactions are possible, such as the control of arousal.

However, these have not been implemented, and we wait to see whether evolution
can discover them for us.
  As well as controlling vital neural systems, biochemistry is used to implement
those systems which are not actually necessary or compulsory within digital organ-
isms, yet which would be expected by the general public. For example a simple
metabolic system is simulated based on the following reactions:
   starch   )    glucose   () glycogen
                 +
                 CO2 + H2 O + energy
  Similarly, a selection of biochemicals and reactions produce the e ects of toxins,
which may be ingested from plants or emitted by the various synthetic `bacte-
ria' which inhabit the environment. These bacteria carry various `antigens', which
invoke `antibody' production in the creatures, causing a very simpli ed immune
response. The bacterial population is allowed to mutate and evolve, o ering the
potential for co-evolution between the population of bacteria and the population
of creatures: new strains of harmful bacteria may occasionally arise through mu-
tation, and rapidly spread through the population of creatures. If this happens,
creatures with a genetic susceptibility to the bacteria may be killed or weakened,
reducing their chances of surviving long enough to reproduce. But any creatures
with a genetically-speci ed resistance or immunity to the bacteria will be more
likely (in relative terms) to reproduce, and so the genetically speci ed resistance
may spread through the creature population, thereby reducing the \ tness" of the
strain of harmful bacteria relative to other strains in the bacterial population. Thus,
shifts in the genetic constitution of one population can trigger genetic shifts in the
other population, and this co-evolutionary interaction can potentially continue in-
de nitely (Cli and Miller, 1995).
  Figure 7 summarizes the processes and interactions within one creature, and
between the creature and its environment.

### 3.4. Genetics
As much as possible of the creature's structure and function are determined by its
genes. Primarily, this genome is provided to allow for inherited characteristics: our
users expect their new-born creatures to show characteristics identi ably drawn
from each parent. However, we have also gone to considerable trouble to ensure
that genomes are capable of evolutionary development, including the introduction
of novel structures brought about by duplicated and mutated genes.
  The genome is a string of bytes, divided into isolated genes by means of `punc-
tuation marks'. Genes of particular types are of characteristic lengths and contain
bytes which are interpreted in speci c ways, although any byte in the genome (other
than gene markers) may safely mutate into any 8-bit value, without fear of crashing
the system.
  The genome forms a single, haploid chromosome. During reproduction, parental
genes are crossed and spliced at gene boundaries. Occasional crossover errors can
introduce gene omissions and duplications. A small number of random mutations to
gene bodies is also applied. To prevent an excessive failure rate due to reproduction
errors in critical genes, each gene is preceded by a header which speci es which
operations (omission, duplication and mutation) may be performed on it. Crossing-
over is performed in such a way that gene linkage is proportional to separation
distance, allowing for linked characteristics such as might be expected (for example,
temperament with facial type). Because the genome is haploid, we have to prevent
useful sex-linked characteristics from being eradicated simply because they were
inherited by a creature of the opposite sex. Therefore, each gene carries the genetic
instructions for both sexes, and when the genes are expressed to form the phenotype,
the individual's sex determines whether the male or the female sex-linked genes are
expressed.
  Each gene's header also contains a value determining its switch-on time. The
genome is re-scanned at intervals, and new genes can be expressed to cater for
changes in a creature's structure, appearance and behavior, for example during
puberty.
  Some of our genes simply code for outward characteristics, in the way we speak of
the \gene for red hair" in humans. However, the vast majority code for structure,
not function. We could not emulate the fact that real genes code only for proteins,
which produce structures, which in turn produce characteristics. However, we have
tried to stay as true as we can to the principle that genotype and phenotype are sep-
arated by several orders of abstraction. Genes in our creatures' genomes therefore
code for structures such as chemo-receptors, reactions and brain lobes, rather than
outward phenomena such as disease-resistance, fearlessness, curiosity, or strength.

---

## 4. Discussion and Conclusions
It is dicult to provide any \results" in this paper, since the project was essentially
an exercise in engineering, rather than science. The overall objective was to create
synthetic, biological agents, whose behavior was suciently life-like to satisfy the
expectations of the general public. In one sense, our results are sales gures: over
100,000 units of the Creatures product were sold in the rst week following the
release in Europe; similarly, more than 100,000 units were sold in the rst quarter
following the US release. At the time of writing, approximately 400,000 units have
been sold worldwide. We take this as evidence of success.
  Certainly, in subjective terms, we have achieved most of our aims: the behavior
of the creatures is dynamically \interesting" and varied and they do indeed appear
to learn. Occasional examples of apparently emergent \social" behavior have been
observed, such as cooperation in playing with a ball, or \chase" scenes resulting
from \unrequited love". However, it is very dicult to establish how much of this is
genuine and how much is conferred by an observer's tendency to anthropomorphism.
The dynamical behavior of the agents and overall environment has been gratifyingly
stable, and con guring a usable genotype has not been a problem, despite requiring
approximately 320 interacting genes, each with several parameters. From that point
of view, our belief that such a complex synthesis of sub-systems was an achievable
aim appears to have been justi ed.
  We believe that Creatures is probably the only commercial product available that
allows home users to interact with arti cial autonomous agents, whose behavior is
controlled by genetically-speci ed neural networks interacting with a genetically-
speci ed biochemical system, and to breed successive generations of those agents.
As the creatures are responsible for coordinating perception and action for extended
periods of time, and for maintaining sucient internal energy to survive and mature
to the point where they are capable of sexual reproduction, it could plausibly be
argued that they are instances of \strong" arti cial life, i.e. that they exhibit the
necessary and sucient conditions to be described as an instance of life. Naturally,
formulating such a list of conditions raises a number of philosophical diculties,
and we do not claim here that the creatures are alive. Rather, we note that the
philosophical debate concerning the possibility of, and requirements for, strong
arti cial life, will be raised in the minds of many of the users of Creatures. For
further discussion of the philosophy of arti cial life, see the collection edited by
Boden (1996). As such, the \general public" will be engaging with arti cial life
technologies in a more complete manner when using Creatures than when using
any other entertainment software with which we are familiar.
  Furthermore, if we assume that each user runs 5 to 10 creatures at a time, then
with sales of 400,000 units there could currently be up to four million creatures
existing in the \cyberspace" provided by the machines of the global Creatures user
community. Continued growth of the global creatures population, to gures mea-
sured in tens of millions, is possible. In this sense, the user community will be
helping to create a \digital biodiversity reserve" or \global digital ecosystem" sim-
ilar to that advocated by T. S. Ray in his ongoing work on NetTierra, a major
global Arti cial Life research experiment (Ray 1994, 1996): this is an issue we dis-
cuss at length in (Cli and Grand, 1998). Already, approximately 200 independent
web-sites have been created by Creatures enthusiasts, several of these concentrate
on \genetic engineering" to create new breeds of creature. If we chose to, we could
monitor the evolution of particular features in groups of creatures: on a local scale
there may be little variation, but national or global comparisons may reveal diver-
gent evolutionary paths. Also, because the creatures can learn within their life-
times, both from humans and from other creatures, it should be possible to study
the spread of \culture" or the emergence of \dialects" as creatures, moved from
machine to machine via electronic mail or web uploads and downloads, teach each
other behaviors or language variants. In this sense, it seems reasonable to consider
the world-wide community of Creatures users as taking part in an international
Arti cial Life science experiment. Hopefully, they are also having fun.

Acknowledgments
Creatures was developed by CyberLife Technology Ltd (while trading under the
name of Millennium Interactive Ltd) and is published in Europe by GT Interactive
and in North America and Japan by Mindscape. The core Arti cial Life techniques
developed for use in Creatures are referred to as CyberLifetm . The CyberLife Web
site is http://www.cyberlife.co.uk
Notes
1. In keeping with standard biology terminology, we refer to a neuron's input-connections as
   `dendrites'.

References
 1. (Anark 1996) Website at http://www.anark.com
 2. (Arbib 1995) M. A. Arbib (editor) The Handbook of Brain Theory and Neural Networks.
    MIT Press.
 3. (Bates 1994) J. Bates, \The role of emotion in believable characters", Communications of
    the ACM 37(7).
 4. (Beer 1995a) R. D. Beer, \On the Dynamics of Small Continuous-Time Recurrent Neural
    Networks", Adaptive Behavior 3(4):471{511.
 5. (Beer 1995b) R. D. Beer, \A Dynamical Systems Perspective on agent-environment interac-
    tion", Arti cial Intelligence 72:173{215.
 6. (Beer 1996) R. D. Beer, \Toward the Evolution of Dynamical Neural Networks for Minimally
    Cognitive Behavior", in (Maes et al 1996) pages 421{429.
    in (Cli et al 1994) pp. 108-117.
 8. (Blumberg 1996) B. Blumberg Old Tricks, New Dogs: Ethology and Interactive Creatures,
    Unpublished PhD Thesis, MIT Media Lab.
 9. (Boden 1996) M. Boden (editor), The Philosophy of Arti cial Life. Oxford University Press.
10. (Brooks and Maes 1994) R. Brooks and P. Maes (editors), ALifeIV: Proceedings of the
    Arti cial Life IV Workshop. MIT Press.
11. (Cli et al 1994) D. Cli , P. Husbands, J.-A. Meyer and S.W. Wilson, (editors) From Animals
    to Animats 3: Proceedings of the 3rd International Conference on the Simulation of Adaptive
    Behavior (SAB94). MIT Press.
12. (Cli and Miller 1995) D. Cli and G. F. Miller, \Tracking the Red Queen: Measurements of
    Adaptive Progress in Co-Evolutionary Simulations". In (Moran et al 1995) pages 200{218.
13. (Cli and Grand 1998) D. Cli and S. Grand, \The `Creatures' Global Digital Ecosystem".
    Manuscript submitted to The Sixth International Workshop on Arti cial Life (ALifeVI).
14. (Coveney and High eld 1995) P. Coveney and R. High eld Frontiers of Complexity. Faber
    and Faber.
15. (Fujitsu 1996) Website at http://www.finfin.com
16. (Grand, Cli , and Malhotra 1997) S. Grand, D. Cli , and A. Malhotra, \Creatures: Arti cial
    Life AutonomousSoftware Agents for Home Entertainment". In W. L. Johnson and B. Hayes-
    Roth, (editors) Proceedings of the First International Conference on Autonomous Agents,
    pages 22{29. ACM Press. Also available as University of Sussex School of Cognitive and
    Computing Sciences Technical Report CSRP434.
17. (Goldberg 1989) D. E. Goldberg, Genetic Algorithms in Search, Optimization, and Machine
    Learning. Addison Wesley.
18. (Hayes-Roth and van Gent 1997) B. Hayes-Roth and R. van Gent, \Story-Making with
    Improvisational Puppets", In W. L. Johnson and B. Hayes-Roth, (editors) Proceedings of
    the First International Conference on Autonomous Agents, pages 1{7. ACM Press.
19. (Husbands and Harvey 1997) P. Husbands and I. Harvey (editors) Proceedings of the Fourth
    European Conference on Arti cial Life (ECAL97). MIT Press.
20. (Kelly 1995) K. Kelly, Out of Control. Fourth Estate.
21. (Koza 1992) J. R. Koza Genetic Programming: On the programming of computers by means
    of natural selection. MIT Press.
22. (Langton and Shimohara 1997) C. Langton and K. Shimohara (editors) Arti cial Life V.
    MIT Press.
23. (Lester and Stone 1997) J. C. Lester and B. A. Stone, \Increasing Believability in Animated
    Pedagogical Agents", In W. L. Johnson and B. Hayes-Roth, (editors) Proceedings of the First
    International Conference on Autonomous Agents, pages 8{15. ACM Press.
24. (Levy 1993) S. Levy Arti cial Life: The Quest for a New Creation. Penguin.
25. (Loyall and Bates 1997) A. B. Loyall and J. Bates, \Personality-Rich Believable Agents
    That Use Language", In W. L. Johnson and B. Hayes-Roth, (editors) Proceedings of the
    First International Conference on Autonomous Agents, pages 106{113. ACM Press.
26. (Maes 1995) P. Maes \Arti cial Life Meets Entertainment: Lifelike Autonomous Agents"
    Communications of the ACM. 38(11):108-114,
27. (Maes et al 1996) P. Maes, M. Mataric, J.-A. Meyer, J. Pollack, and S. W. Wilson, edi-
    tors, From Animals to Animats 4: Proceedings of the 4th International Conference on the
    Simulation of Adaptive Behavior (SAB96). MIT Press.
28. (Maxis 1996) Website at http://www.maxis.com/
29. (Moran et al 1995) F. Moran, A. Moreno, J. J. Merelo, P.Chacon, Advances in Arti cial Life:
    Proceedings of the Third European Conference on Arti cial Life (ECAL95). Springer-Verlag.
30. (PFMagic 1996) Website at http://www.pfmagic.com/
31. (Ray 1996) T. S. Ray \Continuing Report on the Network Tierra Experiment" unpublished
    document available from
     http://www.hip.atr.co.jp/~ray/tierra/netreport/netreport.html
32. (Ray 1994) T. S. Ray \A Proposal To Create Two BioDiversity Reserves: One Digital, and
    One Organic" unpublished document available from
     http://www.hip.atr.co.jp/~ray/pubs/reserves/reserves.html
33. (Reynolds 1987) C. Reynolds, \Flocks, herds and schools: A distributed behavioral model".
    Computer Graphics 21(4):25{34.
34. (Reynolds 1994) C. Reynolds \Evolution of Corridor Following in a Noisy World" in (Cli
    et al 1994).
35. (Rumelhart and McClelland 1986) D. E. Rumelhart and J. L. McClelland (editors) Parallel
    Distributed Processing, Volume 1: Foundations MIT Press.
36. (Sims 1994) K. Sims \Evolving 3D Morphology and Behavior by Competition", in (Brooks
    and Maes 1994) pp.28{39.
37. (Terzopoulos et al 1994) D. Terzopoulos et al. Arti cial shes with autonomous locomotion,
    perception, behavior and learning, in a physical world. In (Brooks and Maes 1994) pp.17{27.
7. (Blumberg 1994) B. Blumberg "Action Selection in Hamsterdam: Lessons from Ethology" in (Cliff et al 1994) pp. 108–117.
8. (Blumberg 1996) B. Blumberg Old Tricks, New Dogs: Ethology and Interactive Creatures, Unpublished PhD Thesis, MIT Media Lab.
9. (Boden 1996) M. Boden (editor), The Philosophy of Artificial Life. Oxford University Press.
10. (Brooks and Maes 1994) R. Brooks and P. Maes (editors), ALifeIV: Proceedings of the Artificial Life IV Workshop. MIT Press.
11. (Cliff et al 1994) D. Cliff, P. Husbands, J.-A. Meyer and S.W. Wilson, (editors) From Animals to Animats 3: Proceedings of the 3rd International Conference on the Simulation of Adaptive Behavior (SAB94). MIT Press.
12. (Cliff and Miller 1995) D. Cliff and G. F. Miller, "Tracking the Red Queen: Measurements of Adaptive Progress in Co-Evolutionary Simulations". In (Morán et al 1995) pages 200–218.
13. (Cliff and Grand 1998) D. Cliff and S. Grand, "The 'Creatures' Global Digital Ecosystem". Manuscript submitted to The Sixth International Workshop on Artificial Life (ALifeVI).
14. (Coveney and Highfield 1995) P. Coveney and R. Highfield Frontiers of Complexity. Faber and Faber.
15. (Fujitsu 1996) Website at http://www.finfin.com
16. (Grand, Cliff, and Malhotra 1997) S. Grand, D. Cliff, and A. Malhotra, "Creatures: Artificial Life Autonomous Software Agents for Home Entertainment". In W. L. Johnson and B. Hayes-Roth, (editors) Proceedings of the First International Conference on Autonomous Agents, pages 22–29. ACM Press. Also available as University of Sussex School of Cognitive and Computing Sciences Technical Report CSRP434.
17. (Goldberg 1989) D. E. Goldberg, Genetic Algorithms in Search, Optimization, and Machine Learning. Addison Wesley.
18. (Hayes-Roth and van Gent 1997) B. Hayes-Roth and R. van Gent, "Story-Making with Improvisational Puppets", In W. L. Johnson and B. Hayes-Roth, (editors) Proceedings of the First International Conference on Autonomous Agents, pages 1–7. ACM Press.
19. (Husbands and Harvey 1997) P. Husbands and I. Harvey (editors) Proceedings of the Fourth European Conference on Artificial Life (ECAL97). MIT Press.
20. (Kelly 1995) K. Kelly, Out of Control. Fourth Estate.
21. (Koza 1992) J. R. Koza Genetic Programming: On the programming of computers by means of natural selection. MIT Press.
