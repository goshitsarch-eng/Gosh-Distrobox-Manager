import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';
import 'package:gosh_distrobox_manager/src/rust/backends/distrobox/distrobox.dart';

class CreateContainerPage extends StatefulWidget {
  final String? preselectedImage;

  const CreateContainerPage({super.key, this.preselectedImage});

  @override
  State<CreateContainerPage> createState() => _CreateContainerPageState();
}

class _CreateContainerPageState extends State<CreateContainerPage> {
  final PageController _pageController = PageController();
  int _currentStep = 0; // 0: Image, 1: Config, 2: Progress

  String? _selectedImage;
  String? _taskId;

  // Form values
  final TextEditingController _nameController = TextEditingController();
  final TextEditingController _homeDirController = TextEditingController();
  final TextEditingController _customImageController = TextEditingController();
  final TextEditingController _searchController = TextEditingController();
  String _searchQuery = '';
  bool _initSystem = false;
  bool _nvidiaSupport = false;
  bool _showAdvanced = false;

  // Volume management
  final List<Volume> _volumes = [];

  @override
  void initState() {
    super.initState();
    if (widget.preselectedImage != null) {
      _selectedImage = widget.preselectedImage;
      _customImageController.text = widget.preselectedImage!;
    }
    // Load available images
    WidgetsBinding.instance.addPostFrameCallback((_) {
      context.read<AppStateProvider>().loadAvailableImages();
    });
  }

  @override
  void dispose() {
    _pageController.dispose();
    _nameController.dispose();
    _homeDirController.dispose();
    _customImageController.dispose();
    _searchController.dispose();
    super.dispose();
  }

  String _getEffectiveImage() {
    if (_customImageController.text.isNotEmpty) {
      return _customImageController.text;
    }
    return _selectedImage ?? '';
  }

  void _goToConfig() {
    final image = _getEffectiveImage();
    if (image.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Please select an image')));
      return;
    }

    // Generate a default name from the image
    if (_nameController.text.isEmpty) {
      final imageName = _extractImageName(image).toLowerCase();
      _nameController.text = '$imageName-container';
    }

    setState(() {
      _currentStep = 1;
    });
    _pageController.animateToPage(
      1,
      duration: const Duration(milliseconds: 300),
      curve: Curves.easeInOut,
    );
  }

  Future<void> _startCreation() async {
    if (_nameController.text.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Please enter a container name')));
      return;
    }

    setState(() {
      _currentStep = 2;
    });
    _pageController.animateToPage(
      2,
      duration: const Duration(milliseconds: 300),
      curve: Curves.easeInOut,
    );

    // Create the container
    final args = CreateArgs(
      name: CreateArgName(field0: _nameController.text),
      image: _getEffectiveImage(),
      init: _initSystem,
      nvidia: _nvidiaSupport,
      homePath: _homeDirController.text.isNotEmpty ? _homeDirController.text : null,
      volumes: _volumes,
    );

    final appState = context.read<AppStateProvider>();
    final taskId = await appState.createContainer(args);

    if (taskId != null && mounted) {
      setState(() {
        _taskId = taskId;
      });
    } else if (mounted) {
      // Failed to start creation
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(appState.actionError ?? 'Failed to create container')),
      );
    }
  }

  void _goBack() {
    if (_currentStep > 0 && _currentStep < 2) {
      setState(() {
        _currentStep--;
      });
      _pageController.animateToPage(
        _currentStep,
        duration: const Duration(milliseconds: 300),
        curve: Curves.easeInOut,
      );
    } else if (_currentStep == 0) {
      Navigator.of(context).pop();
    }
  }

  String _extractImageName(String imageUrl) {
    final parts = imageUrl.split('/');
    final lastPart = parts.last.split(':').first;
    if (lastPart.isEmpty) return 'container';
    return lastPart[0].toUpperCase() + lastPart.substring(1);
  }

  IconData _getDistroIcon(String imageName) {
    final nameLower = imageName.toLowerCase();
    if (nameLower.contains('ubuntu')) return Icons.circle;
    if (nameLower.contains('fedora')) return Icons.filter_vintage;
    if (nameLower.contains('arch')) return Icons.architecture;
    if (nameLower.contains('debian')) return Icons.donut_large;
    if (nameLower.contains('alpine')) return Icons.landscape;
    if (nameLower.contains('centos') || nameLower.contains('rocky')) {
      return Icons.shield;
    }
    if (nameLower.contains('opensuse') || nameLower.contains('suse')) {
      return Icons.pets;
    }
    return Icons.dns;
  }

  Color _getDistroColor(String imageName) {
    final nameLower = imageName.toLowerCase();
    if (nameLower.contains('ubuntu')) return Colors.orange;
    if (nameLower.contains('fedora')) return Colors.blue;
    if (nameLower.contains('arch')) return Colors.cyan;
    if (nameLower.contains('debian')) return Colors.red;
    if (nameLower.contains('alpine')) return Colors.blueGrey;
    if (nameLower.contains('centos') || nameLower.contains('rocky')) {
      return Colors.green;
    }
    if (nameLower.contains('opensuse') || nameLower.contains('suse')) {
      return Colors.lightGreen;
    }
    return Colors.purple;
  }

  List<String> _filterImages(List<String> images) {
    if (_searchQuery.isEmpty) return images;
    final query = _searchQuery.toLowerCase();
    return images.where((image) => image.toLowerCase().contains(query)).toList();
  }

  void _addVolume() {
    showDialog(
      context: context,
      builder: (context) => _AddVolumeDialog(
        onAdd: (volume) {
          setState(() {
            _volumes.add(volume);
          });
        },
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Create Container'),
        leading: IconButton(
          icon: const Icon(Icons.close),
          onPressed: () => Navigator.of(context).pop(),
        ),
      ),
      body: Column(
        children: [
          // Stepper
          Container(
            padding: const EdgeInsets.symmetric(vertical: 24),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                _buildStepDot(0, active: _currentStep == 0, completed: _currentStep > 0),
                const SizedBox(width: 8),
                _buildStepDot(1, active: _currentStep == 1, completed: _currentStep > 1),
                const SizedBox(width: 8),
                _buildStepDot(2, active: _currentStep == 2),
              ],
            ),
          ),

          Expanded(
            child: PageView(
              controller: _pageController,
              physics: const NeverScrollableScrollPhysics(),
              children: [
                _buildImageSelectionStep(),
                _buildConfigurationStep(),
                _buildProgressStep(),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildStepDot(int index, {bool active = false, bool completed = false}) {
    Color color = Theme.of(context).dividerColor;
    double width = 8;

    if (completed) {
      color = Theme.of(context).primaryColor;
    } else if (active) {
      color = Theme.of(context).primaryColor;
      width = 32;
    }

    return AnimatedContainer(
      duration: const Duration(milliseconds: 300),
      height: 8,
      width: width,
      decoration: BoxDecoration(
        color: color,
        borderRadius: BorderRadius.circular(4),
      ),
    );
  }

  Widget _buildImageSelectionStep() {
    return Consumer<AppStateProvider>(
      builder: (context, appState, child) {
        final filteredImages = _filterImages(appState.availableImages);

        return Column(
          children: [
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Column(
                children: [
                  Text('Select Image',
                      style: Theme.of(context)
                          .textTheme
                          .headlineMedium
                          ?.copyWith(fontWeight: FontWeight.bold)),
                  const SizedBox(height: 8),
                  Text('Choose a Linux distribution for your container.',
                      style: TextStyle(color: Theme.of(context).hintColor)),
                  const SizedBox(height: 16),
                  SearchBar(
                    controller: _searchController,
                    leading: const Icon(Icons.search),
                    hintText: 'Search distributions...',
                    onChanged: (value) {
                      setState(() => _searchQuery = value);
                    },
                    elevation: WidgetStateProperty.all(0),
                    backgroundColor: WidgetStateProperty.resolveWith((states) {
                      return Theme.of(context).brightness == Brightness.dark
                          ? Colors.grey[800]
                          : Colors.white;
                    }),
                  ),
                ],
              ),
            ),
            Expanded(
              child: appState.isLoadingImages
                  ? const Center(child: CircularProgressIndicator())
                  : filteredImages.isEmpty
                      ? Center(
                          child: Column(
                            mainAxisAlignment: MainAxisAlignment.center,
                            children: [
                              Icon(Icons.album_outlined,
                                  size: 48, color: Theme.of(context).hintColor),
                              const SizedBox(height: 16),
                              Text('No images available',
                                  style: TextStyle(
                                      color: Theme.of(context).hintColor)),
                              const SizedBox(height: 8),
                              FilledButton.icon(
                                onPressed: () =>
                                    appState.loadAvailableImages(),
                                icon: const Icon(Icons.refresh),
                                label: const Text('Refresh'),
                              ),
                            ],
                          ),
                        )
                      : GridView.builder(
                          gridDelegate:
                              const SliverGridDelegateWithFixedCrossAxisCount(
                            crossAxisCount: 2,
                            mainAxisSpacing: 16,
                            crossAxisSpacing: 16,
                            childAspectRatio: 1.0,
                          ),
                          padding: const EdgeInsets.all(16),
                          itemCount: filteredImages.length,
                          itemBuilder: (context, index) {
                            final image = filteredImages[index];
                            return _buildDistroCard(image);
                          },
                        ),
            ),
            // Custom Image Input
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Divider(),
                  const Text('Custom Image URL',
                      style: TextStyle(fontWeight: FontWeight.w600)),
                  const SizedBox(height: 8),
                  TextField(
                    controller: _customImageController,
                    decoration: const InputDecoration(
                      hintText: 'e.g. docker.io/library/ubuntu:22.04',
                      border: OutlineInputBorder(
                          borderRadius: BorderRadius.all(Radius.circular(12))),
                    ),
                    onChanged: (value) {
                      if (value.isNotEmpty) {
                        setState(() => _selectedImage = null);
                      }
                    },
                  ),
                  const SizedBox(height: 8),
                  Text('Enter a manual image path or registry URL.',
                      style: TextStyle(
                          fontSize: 12, color: Theme.of(context).hintColor)),
                ],
              ),
            ),
            // Actions
            Container(
              padding: const EdgeInsets.all(16),
              child: Row(
                children: [
                  Expanded(
                    child: OutlinedButton(
                      onPressed: () => Navigator.of(context).pop(),
                      style: OutlinedButton.styleFrom(
                        padding: const EdgeInsets.symmetric(vertical: 16),
                        shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(12)),
                      ),
                      child: const Text('Cancel'),
                    ),
                  ),
                  const SizedBox(width: 16),
                  Expanded(
                    child: FilledButton(
                      onPressed: _goToConfig,
                      style: FilledButton.styleFrom(
                        padding: const EdgeInsets.symmetric(vertical: 16),
                        shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(12)),
                      ),
                      child: const Text('Next'),
                    ),
                  ),
                ],
              ),
            ),
          ],
        );
      },
    );
  }

  Widget _buildDistroCard(String imageUrl) {
    final name = _extractImageName(imageUrl);
    final isSelected = _selectedImage == imageUrl;
    final color = _getDistroColor(imageUrl);
    final icon = _getDistroIcon(imageUrl);

    return InkWell(
      onTap: () {
        setState(() {
          _selectedImage = imageUrl;
          _customImageController.clear();
        });
      },
      child: Container(
        decoration: BoxDecoration(
          color: Theme.of(context).cardColor,
          borderRadius: BorderRadius.circular(16),
          border: Border.all(
            color: isSelected
                ? Theme.of(context).primaryColor
                : Theme.of(context).dividerColor,
            width: isSelected ? 2 : 1,
          ),
        ),
        child: Stack(
          children: [
            Center(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Container(
                    width: 64,
                    height: 64,
                    decoration: BoxDecoration(
                      color: color.withOpacity(0.1),
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: Icon(icon, size: 32, color: color),
                  ),
                  const SizedBox(height: 12),
                  Text(name, style: const TextStyle(fontWeight: FontWeight.bold)),
                ],
              ),
            ),
            if (isSelected)
              Positioned(
                top: 8,
                right: 8,
                child: Icon(Icons.check_circle, color: Theme.of(context).primaryColor),
              ),
          ],
        ),
      ),
    );
  }

  Widget _buildConfigurationStep() {
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Column(
            children: [
              Text('Configuration',
                  style: Theme.of(context)
                      .textTheme
                      .headlineMedium
                      ?.copyWith(fontWeight: FontWeight.bold)),
              const SizedBox(height: 8),
              Text('Step 2 of 3: System settings',
                  style: TextStyle(color: Theme.of(context).hintColor)),
            ],
          ),
        ),
        Expanded(
          child: ListView(
            padding: const EdgeInsets.all(16),
            children: [
              // Selected Image Preview
              Container(
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.primary.withOpacity(0.1),
                  borderRadius: BorderRadius.circular(12),
                ),
                child: Row(
                  children: [
                    Icon(_getDistroIcon(_getEffectiveImage()),
                        color: Theme.of(context).colorScheme.primary),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text('Selected Image',
                              style: TextStyle(
                                  fontSize: 12,
                                  color: Theme.of(context).hintColor)),
                          Text(_getEffectiveImage(),
                              style:
                                  const TextStyle(fontWeight: FontWeight.bold),
                              overflow: TextOverflow.ellipsis),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 24),

              // Container Name
              const Text('Container Name',
                  style: TextStyle(fontWeight: FontWeight.w500)),
              const SizedBox(height: 8),
              TextField(
                controller: _nameController,
                decoration: const InputDecoration(
                  hintText: 'e.g. arch-dev-box',
                  border: OutlineInputBorder(
                      borderRadius: BorderRadius.all(Radius.circular(8))),
                ),
              ),
              const SizedBox(height: 24),

              // Init System
              ListTile(
                contentPadding: EdgeInsets.zero,
                title: const Text('Init System',
                    style: TextStyle(fontWeight: FontWeight.w500)),
                subtitle: const Text('Run an init system inside the container'),
                trailing: Switch(
                  value: _initSystem,
                  onChanged: (v) => setState(() => _initSystem = v),
                ),
              ),

              // NVIDIA Support
              ListTile(
                contentPadding: EdgeInsets.zero,
                title: const Text('NVIDIA GPU Support',
                    style: TextStyle(fontWeight: FontWeight.w500)),
                subtitle: const Text('Enable NVIDIA GPU passthrough'),
                trailing: Switch(
                  value: _nvidiaSupport,
                  onChanged: (v) => setState(() => _nvidiaSupport = v),
                ),
              ),
              const Divider(),

              // Advanced Options
              ExpansionTile(
                title: const Text('Advanced Options',
                    style: TextStyle(fontWeight: FontWeight.w600)),
                initiallyExpanded: _showAdvanced,
                onExpansionChanged: (v) => setState(() => _showAdvanced = v),
                children: [
                  const SizedBox(height: 16),
                  const Align(
                      alignment: Alignment.centerLeft,
                      child: Text('Home Directory',
                          style: TextStyle(fontWeight: FontWeight.w500))),
                  const SizedBox(height: 8),
                  TextField(
                    controller: _homeDirController,
                    decoration: const InputDecoration(
                      hintText: '/home/user/containers/my-box',
                      suffixIcon: Icon(Icons.folder_open),
                      border: OutlineInputBorder(
                          borderRadius: BorderRadius.all(Radius.circular(8))),
                    ),
                  ),
                  const SizedBox(height: 16),

                  // Volume Mounts
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      const Text('Volume Mounts',
                          style: TextStyle(fontWeight: FontWeight.w500)),
                      TextButton.icon(
                        onPressed: _addVolume,
                        icon: const Icon(Icons.add, size: 18),
                        label: const Text('Add'),
                      ),
                    ],
                  ),
                  if (_volumes.isEmpty)
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: 8),
                      child: Text('No volumes configured',
                          style: TextStyle(color: Theme.of(context).hintColor)),
                    )
                  else
                    ..._volumes.asMap().entries.map((entry) {
                      final index = entry.key;
                      final volume = entry.value;
                      return ListTile(
                        contentPadding: EdgeInsets.zero,
                        leading: const Icon(Icons.folder),
                        title: Text('${volume.hostPath} -> ${volume.containerPath}'),
                        subtitle: volume.mode == VolumeMode.readOnly
                            ? const Text('Read-only')
                            : null,
                        trailing: IconButton(
                          icon: const Icon(Icons.close, size: 18),
                          onPressed: () {
                            setState(() => _volumes.removeAt(index));
                          },
                        ),
                      );
                    }),
                  const SizedBox(height: 16),
                ],
              ),
            ],
          ),
        ),
        // Actions
        Container(
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
            color: Theme.of(context).scaffoldBackgroundColor.withOpacity(0.9),
            border: Border(
                top: BorderSide(color: Theme.of(context).dividerColor)),
          ),
          child: Row(
            children: [
              Expanded(
                child: OutlinedButton(
                  onPressed: _goBack,
                  style: OutlinedButton.styleFrom(
                    padding: const EdgeInsets.symmetric(vertical: 16),
                    shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(12)),
                  ),
                  child: const Text('Back'),
                ),
              ),
              const SizedBox(width: 16),
              Expanded(
                flex: 2,
                child: FilledButton(
                  onPressed: _startCreation,
                  style: FilledButton.styleFrom(
                    padding: const EdgeInsets.symmetric(vertical: 16),
                    shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(12)),
                  ),
                  child: const Text('Create'),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _buildProgressStep() {
    return Consumer<AppStateProvider>(
      builder: (context, appState, child) {
        final task = _taskId != null ? appState.getTask(_taskId!) : null;
        final output = task?.output ?? [];
        final hasFailed = task?.failed ??
            output.any((line) =>
                line.toLowerCase().contains('error') ||
                line.toLowerCase().contains('failed'));
        final isComplete = task?.completed ??
            (output.isNotEmpty &&
                (output.last.toLowerCase().contains('completed') ||
                    output.last.toLowerCase().contains('failed')));

        return Column(
          children: [
            const SizedBox(height: 32),
            // Progress Indicator
            Stack(
              alignment: Alignment.center,
              children: [
                SizedBox(
                  width: 120,
                  height: 120,
                  child: CircularProgressIndicator(
                    value: isComplete ? 1.0 : null,
                    strokeWidth: 8,
                    backgroundColor:
                        Theme.of(context).dividerColor.withOpacity(0.2),
                    color: hasFailed
                        ? Colors.red
                        : isComplete
                            ? Colors.green
                            : null,
                  ),
                ),
                Icon(
                  hasFailed
                      ? Icons.error
                      : isComplete
                          ? Icons.check_circle
                          : Icons.dns,
                  size: 48,
                  color: hasFailed
                      ? Colors.red
                      : isComplete
                          ? Colors.green
                          : Theme.of(context).colorScheme.primary,
                ),
              ],
            ),
            const SizedBox(height: 24),
            Text(
              hasFailed
                  ? 'Creation Failed'
                  : isComplete
                      ? 'Container Created!'
                      : 'Creating Container...',
              style: const TextStyle(fontSize: 18, fontWeight: FontWeight.bold),
            ),
            const SizedBox(height: 8),
            Text(
              hasFailed
                  ? 'Check the output below for details'
                  : isComplete
                      ? '${_nameController.text} is ready to use'
                      : 'This may take a few minutes',
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
            const SizedBox(height: 24),

            // Console Output
            Expanded(
              child: Container(
                margin: const EdgeInsets.symmetric(horizontal: 16),
                decoration: BoxDecoration(
                  color: Theme.of(context).brightness == Brightness.dark
                      ? Colors.black26
                      : Colors.grey[100],
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(color: Theme.of(context).dividerColor),
                ),
                child: Column(
                  children: [
                    Container(
                      padding: const EdgeInsets.symmetric(
                          horizontal: 16, vertical: 8),
                      decoration: BoxDecoration(
                        color:
                            Theme.of(context).dividerColor.withOpacity(0.1),
                        border: Border(
                            bottom: BorderSide(
                                color: Theme.of(context).dividerColor)),
                      ),
                      child: const Row(
                        mainAxisAlignment: MainAxisAlignment.spaceBetween,
                        children: [
                          Text('CONSOLE OUTPUT',
                              style: TextStyle(
                                  fontSize: 10,
                                  fontWeight: FontWeight.bold,
                                  letterSpacing: 1.0)),
                          Icon(Icons.terminal, size: 16),
                        ],
                      ),
                    ),
                    Expanded(
                      child: output.isEmpty
                          ? Center(
                              child: Text('Waiting for output...',
                                  style: TextStyle(
                                      color: Theme.of(context).hintColor)))
                          : ListView.builder(
                              padding: const EdgeInsets.all(16),
                              itemCount: output.length,
                              itemBuilder: (context, index) {
                                final line = output[index];
                                Color textColor = Theme.of(context)
                                    .textTheme
                                    .bodyMedium!
                                    .color!;
                                if (line.startsWith('Error') ||
                                    line.toLowerCase().contains('error')) {
                                  textColor = Colors.red;
                                } else if (line.startsWith('>') ||
                                    line.startsWith('->')) {
                                  textColor =
                                      Theme.of(context).colorScheme.primary;
                                }
                                return Text(
                                  line,
                                  style: TextStyle(
                                    fontFamily: 'monospace',
                                    fontSize: 12,
                                    color: textColor,
                                  ),
                                );
                              },
                            ),
                    ),
                  ],
                ),
              ),
            ),

            // Actions
            Container(
              padding: const EdgeInsets.all(16),
              child: Row(
                children: [
                  if (!isComplete && !hasFailed) ...[
                    Expanded(
                      child: OutlinedButton(
                        onPressed: () async {
                          if (_taskId != null) {
                            await appState.cancelTask(_taskId!);
                          }
                          if (mounted) Navigator.of(context).pop();
                        },
                        style: OutlinedButton.styleFrom(
                          padding: const EdgeInsets.symmetric(vertical: 16),
                          shape: RoundedRectangleBorder(
                              borderRadius: BorderRadius.circular(12)),
                        ),
                        child: const Text('Cancel'),
                      ),
                    ),
                  ],
                  if (isComplete || hasFailed) ...[
                    Expanded(
                      child: FilledButton(
                        onPressed: () {
                          appState.refresh();
                          Navigator.of(context).pop();
                        },
                        style: FilledButton.styleFrom(
                          padding: const EdgeInsets.symmetric(vertical: 16),
                          shape: RoundedRectangleBorder(
                              borderRadius: BorderRadius.circular(12)),
                          backgroundColor:
                              hasFailed ? Colors.red : Colors.green,
                        ),
                        child: Text(hasFailed ? 'Close' : 'Done'),
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ],
        );
      },
    );
  }
}

// Dialog for adding volume mounts
class _AddVolumeDialog extends StatefulWidget {
  final void Function(Volume volume) onAdd;

  const _AddVolumeDialog({required this.onAdd});

  @override
  State<_AddVolumeDialog> createState() => _AddVolumeDialogState();
}

class _AddVolumeDialogState extends State<_AddVolumeDialog> {
  final _hostPathController = TextEditingController();
  final _containerPathController = TextEditingController();
  bool _readOnly = false;

  @override
  void dispose() {
    _hostPathController.dispose();
    _containerPathController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Add Volume Mount'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            controller: _hostPathController,
            decoration: const InputDecoration(
              labelText: 'Host Path',
              hintText: '/home/user/projects',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 16),
          TextField(
            controller: _containerPathController,
            decoration: const InputDecoration(
              labelText: 'Container Path',
              hintText: '/projects',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 16),
          CheckboxListTile(
            title: const Text('Read-only'),
            value: _readOnly,
            onChanged: (v) => setState(() => _readOnly = v ?? false),
            contentPadding: EdgeInsets.zero,
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () {
            if (_hostPathController.text.isEmpty ||
                _containerPathController.text.isEmpty) {
              return;
            }
            widget.onAdd(Volume(
              hostPath: _hostPathController.text,
              containerPath: _containerPathController.text,
              mode: _readOnly ? VolumeMode.readOnly : null,
            ));
            Navigator.pop(context);
          },
          child: const Text('Add'),
        ),
      ],
    );
  }
}
