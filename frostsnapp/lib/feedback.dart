import 'package:flutter/material.dart';
import 'package:frostsnapp/ffi.dart';
import 'package:http/http.dart' as http;

class FeedbackPage extends StatelessWidget {
  const FeedbackPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Alpha Feedback'),
      ),
      body: Padding(
        padding: const EdgeInsets.all(8.0),
        child: FeedbackForm(),
      ),
    );
  }
}

class FeedbackForm extends StatefulWidget {
  const FeedbackForm({Key? key}) : super(key: key);

  @override
  _FeedbackFormState createState() => _FeedbackFormState();
}

class _FeedbackFormState extends State<FeedbackForm> {
  final TextEditingController _feedbackController = TextEditingController();
  bool _isLoading = false;
  String _resultMessage = '';
  bool _resultSuccess = false;

  Future<void> _submitFeedback() async {
    final feedback = _feedbackController.text;
    if (feedback.isEmpty) {
      setState(() {
        _resultMessage = 'Feedback cannot be empty.';
      });
      return;
    }

    setState(() {
      _isLoading = true;
      _resultMessage = '';
    });

    final token = api.randomSecpPointForToken();
    final url = Uri.parse(
        'https://feedback.frostsnap.com/submit?feedback=$feedback&token=$token');
    final response = await http.get(url);

    setState(() {
      _isLoading = false;
      if (response.statusCode == 200) {
        _resultMessage = 'Thank you!!';
        _resultSuccess = true;
        _feedbackController.clear();
      } else {
        _resultSuccess = false;
        _resultMessage =
            'Failed to submit feedback. Please try again.\n${response.body}';
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final screenWidth = MediaQuery.of(context).size.width;
    const double maxLogoWidth = 300;
    final double logoWidth =
        screenWidth > 600 ? maxLogoWidth * 1.5 : maxLogoWidth;

    return Column(
      mainAxisAlignment: MainAxisAlignment.center,
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Image.asset(
          'assets/frostsnap-logo-boxed.png',
          width: logoWidth,
          fit: BoxFit.contain,
        ),
        SizedBox(height: 20),
        Text(
          'Thanks for taking part in the future of Bitcoin self-custody.',
          style: TextStyle(fontSize: 16),
        ),
        const SizedBox(height: 8),
        Divider(),
        const SizedBox(height: 8),
        Text('If you could have any feature right now, what would it be?'),
        const SizedBox(height: 8),
        Text('What is frustrating you about the app or devices?'),
        const SizedBox(height: 8),
        Text('Other thoughts/ideas/suggestions?'),
        const SizedBox(height: 8),
        SizedBox(
          width: logoWidth,
          child: TextField(
            controller: _feedbackController,
            minLines: 2,
            maxLines: 6,
            style: TextStyle(
              backgroundColor: Colors.white,
            ),
            decoration: InputDecoration(
              hintText: 'Enter your feedback here...',
              border: OutlineInputBorder(),
            ),
          ),
        ),
        const SizedBox(height: 8),
        SizedBox(
          width: logoWidth,
          child: ElevatedButton(
            onPressed: _isLoading ? null : _submitFeedback,
            child: _isLoading
                ? CircularProgressIndicator(color: Colors.white)
                : Text('Submit'),
          ),
        ),
        const SizedBox(height: 8),
        if (_resultMessage.isNotEmpty)
          Text(
            _resultMessage,
            style: TextStyle(color: _resultSuccess ? Colors.green : Colors.red),
          ),
      ],
    );
  }
}

void main() {
  runApp(MaterialApp(home: FeedbackPage()));
}
